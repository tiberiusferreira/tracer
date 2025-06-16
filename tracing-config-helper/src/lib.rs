//! This serves as a unified config for projects
//! It outputs pretty logs to the console stdout and stderr,
//! but also exports traces to a collector
//!

use crate::io_provider::execution_recorder::{
    DataCollector, GLOBAL_DATA_COLLECTOR, get_global_collector,
};
pub use api_structs::ServiceId;
use api_structs::instance::registration::RegistrationResponse;
use base64::Engine;
use pprof::ProfilerGuard;
pub use print_debugging::print_if_dbg;
use std::fmt::Debug;
use std::io::Write;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tracked_error::error_chain_to_pretty_formatted;
use uuid::Uuid;

pub mod io_provider;
pub use api_structs::instance::update::{ExecutionRecording, ReplayData};
mod print_debugging;
mod server_connection;

#[derive(Debug, Clone)]
pub struct TracerConfig {
    /// Where to send data to, should not contain a trailing /
    pub collector_url: String,
    pub service_id: ServiceId,
    /// How long to wait for when exporting data before timing out
    pub export_timeout: Duration,
    /// How long to wait between exports. A short duration will flood the collector and a long one will cause the
    /// export buffers to fill up. Stats are also exported on this schedule.
    pub duration_between_exports: Duration,
    pub min_duration_between_profile_exports: Duration,
}

impl TracerConfig {
    pub fn new(service_id: ServiceId, collector_url: String) -> TracerConfig {
        TracerConfig {
            collector_url,
            export_timeout: Duration::from_secs(60),
            duration_between_exports: Duration::from_secs(2),
            min_duration_between_profile_exports: Duration::from_secs(10 * 60),
            service_id,
        }
    }
    pub fn with_export_timeout(mut self, duration: Duration) -> Self {
        self.export_timeout = duration;
        self
    }
    pub fn with_sleep_between_exports(mut self, duration: Duration) -> Self {
        assert!(
            duration.as_secs() >= 2,
            "Sleep between exports needs to be at least 2s to not flood collector"
        );
        self.duration_between_exports = duration;
        self
    }
}

pub struct TracerHandle {
    pub thread_handle: std::thread::JoinHandle<()>,
    pub export_now_requester: ExportNowRequester,
}

pub async fn setup_tracer_client_in_background_or_panic(config: TracerConfig) -> TracerHandle {
    GLOBAL_DATA_COLLECTOR.set(DataCollector::new()).unwrap();
    println!("Starting up using: {config:#?}");
    // we start a new thread and runtime so it can still get data and debug issues involving the main program async
    // runtime starved from CPU time.
    let (s, r) = tokio::sync::oneshot::channel();
    let thread_handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("tracer_thread")
            .build()
            .expect("runtime to be able to start");

        runtime.block_on(async {
            // we use a local set so tasks don't have to implement Send
            tokio::task::LocalSet::new()
                .run_until(async {
                    let tracer_tasks = setup_tracer_client_or_panic_impl(config).await;
                    s.send(tracer_tasks.export_now_request_sender.clone())
                        .unwrap();
                    tracer_tasks.wait_or_panic().await;
                })
                .await;
        });
    });
    let export_now_requester = r.await.expect("initialization to work");
    println!("Tracer fully initialized");
    TracerHandle {
        thread_handle,
        export_now_requester,
    }
}

struct TracerTasks {
    // sse_task: tokio::task::JoinHandle<()>,
    trace_export_task: tokio::task::JoinHandle<()>,
    export_now_request_sender: ExportNowRequester,
}

impl TracerTasks {
    pub async fn wait_or_panic(self) {
        let _res = futures::try_join!(self.trace_export_task).unwrap();
    }
}

struct FlushRequest {
    respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
}

impl FlushRequest {
    fn new() -> (
        tokio::sync::oneshot::Receiver<Result<(), String>>,
        FlushRequest,
    ) {
        let (sender, receiver) = tokio::sync::oneshot::channel::<Result<(), String>>();
        (receiver, Self { respond_to: sender })
    }
}

/// Used for request immediate exporting of current data. This is useful for when the
/// program is about to exit or during tests
#[derive(Debug, Clone)]
pub struct ExportNowRequester {
    sender_channel: Sender<FlushRequest>,
}

impl Drop for ExportNowRequester {
    fn drop(&mut self) {
        println!("Trying to export last data");
        if let Err(e) = self.try_export_dont_wait_result() {
            println!("{e:#?}");
        }
        std::thread::sleep(Duration::new(5, 0));
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum FlushError {
    #[error("Error sending request, subscriber receiving channel is closed or blocked")]
    ChannelClosedBeforeSend,
    #[error("Timeout waiting for response")]
    Timeout,
    #[error("Subscriber receiver channel closed before we got a response")]
    ChannelClosedAfterSend,
    #[error("Data was sent, but we got an error back from Tracer Backend: {0}")]
    TracerBackend(String),
}

impl ExportNowRequester {
    fn new() -> (Receiver<FlushRequest>, ExportNowRequester) {
        let (sender, receiver) = tokio::sync::mpsc::channel::<FlushRequest>(1);
        (
            receiver,
            Self {
                sender_channel: sender,
            },
        )
    }
    pub fn try_export_dont_wait_result(&self) -> Result<(), FlushError> {
        let (_receiver, request) = FlushRequest::new();
        self.sender_channel
            .try_send(request)
            .map_err(|_e| FlushError::ChannelClosedBeforeSend)?;
        Ok(())
    }
    pub async fn export(&self, timeout: Duration) -> Result<(), FlushError> {
        let (receiver, request) = FlushRequest::new();
        self.sender_channel
            .try_send(request)
            .map_err(|_e| FlushError::ChannelClosedBeforeSend)?;
        let flush = tokio::time::timeout(timeout, receiver)
            .await
            .map_err(|_e| FlushError::Timeout)?
            .map_err(|_e| FlushError::ChannelClosedAfterSend)?
            .map_err(|e| FlushError::TracerBackend(e))?;
        Ok(flush)
    }
}

fn start_cpu_profiler() -> ProfilerGuard<'static> {
    pprof::ProfilerGuardBuilder::default()
        // how many times per second to profile
        .frequency(10)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .expect("to be able to start profiler")
}

async fn registration_loop(
    reqwest_client: &reqwest::Client,
    collector_url: &str,
    service_id: ServiceId,
) -> RegistrationResponse {
    loop {
        println!("Sending tracer registration request");
        match server_connection::instance_registration::register_instance(
            &reqwest_client,
            collector_url,
            &service_id,
            Duration::from_secs(10),
        )
        .await
        {
            Ok(registration_response) => return registration_response,
            Err(err) => {
                let err_str = error_chain_to_pretty_formatted(&err);
                let sleep_seconds = 60;
                println!(
                    "Registration failure: {} - sleeping {sleep_seconds}s",
                    err_str
                );
                tokio::time::sleep(Duration::from_secs(sleep_seconds)).await;
            }
        }
    }
}
async fn setup_tracer_client_or_panic_impl(config: TracerConfig) -> TracerTasks {
    let reqwest_client = reqwest::ClientBuilder::new()
        .build()
        .expect("reqwest client to be able to be created");
    let registration_response = registration_loop(
        &reqwest_client,
        &config.collector_url,
        config.service_id.clone(),
    )
    .await;
    println!("registered with collector");
    let (export_now_request_receiver, export_now_request_sender) = ExportNowRequester::new();
    let cpu_profiler_guard = start_cpu_profiler();

    let trace_export_task = tokio::task::spawn_local(trace_export_loop(
        reqwest_client,
        config,
        cpu_profiler_guard,
        export_now_request_receiver,
        registration_response.instance_id,
    ));
    TracerTasks {
        trace_export_task,
        export_now_request_sender,
    }
}

async fn trace_export_loop(
    client: reqwest::Client,
    config: TracerConfig,
    profiler_guard: ProfilerGuard<'static>,
    mut flush_request_receiver: Receiver<FlushRequest>,
    instance_id: Uuid,
) {
    let context = "trace_export_task";
    let min_wait_duration_between_profile_exports = config.min_duration_between_profile_exports;
    let mut time_last_profile_export = std::time::Instant::now();
    println!(
        "{}s between exports",
        config.duration_between_exports.as_secs()
    );
    loop {
        let period_time_secs = config.duration_between_exports;
        print_if_dbg(
            context,
            format!(
                "Sleeping until next export ({}s) or until flush request",
                period_time_secs.as_secs()
            ),
        );
        let flush_request = tokio::select! {
            () = tokio::time::sleep(period_time_secs) => {
                print_if_dbg(context, "Slept");
                None
            },
            received_val = flush_request_receiver.recv() => {
                match received_val{
                    Some(flush_request) => {
                        print_if_dbg(context, "Got flush request");
                        Some(flush_request)
                    }
                    None => {
                        print_if_dbg(context, "Flush request channel is closed, sleeping");
                        tokio::time::sleep(period_time_secs).await;
                        None
                    }
                }
            },
        };
        print_if_dbg(context, "Checking for new events");

        let should_export_profile = (time_last_profile_export.elapsed()
            > min_wait_duration_between_profile_exports)
            || flush_request.is_some();
        let cpu_profile_base64 = if should_export_profile {
            println!("exporting profile");
            time_last_profile_export = std::time::Instant::now();
            let mut profile_data = Vec::new();
            profiler_guard
                .report()
                .build()
                .expect("profile creation to work")
                .flamegraph(&mut profile_data)
                .expect("profile flamegraph generation to work");
            let profile_data_base64 =
                base64::engine::general_purpose::STANDARD_NO_PAD.encode(&profile_data);
            Some(profile_data_base64)
        } else {
            None
        };
        let execution_recording = get_global_collector().get_all_pruning();
        let export_data = api_structs::instance::update::InstanceSnapshot {
            instance_id,
            execution_recordings: execution_recording,
            cpu_profile_base64,
        };
        print_if_dbg(context, format!("Export data: {:#?}", export_data));
        let export_data_json =
            serde_json::to_string(&export_data).expect("export data to be serializable");
        loop {
            print_if_dbg(context, "attempting export");
            tokio::time::sleep(period_time_secs).await;

            match server_connection::instance_update_sender::export_instance_update(
                &client,
                &config.collector_url,
                &export_data_json,
                config.export_timeout,
                config.service_id.name.clone(),
            )
            .await
            {
                Ok(()) => {
                    break;
                }
                Err(err) => {
                    let err = error_chain_to_pretty_formatted(err);
                    println!("{context} - {err}");
                    let file_path = "./data_being_exported.json";
                    let mut file = std::fs::File::create(file_path).expect("failed to create file");
                    println!(
                        "Data being exported size: {}, saved to file {file_path}",
                        export_data_json.len()
                    );
                    file.write_all(export_data_json.as_bytes())
                        .expect("failed to write to file");
                    drop(file);
                    let sleep_sec = Duration::from_secs(10);
                    println!("sleeping 10s");
                    tokio::time::sleep(sleep_sec).await;
                }
            };
        }
        flush_request.map(|f| match f.respond_to.send(Ok(())) {
            Ok(_) => {
                println!("Responded to flush request");
            }
            Err(e) => {
                println!(
                    "Error responding to flush request. Had flush request output: {:#?}",
                    e
                );
            }
        });
    }
}
