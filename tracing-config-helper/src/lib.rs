//! This serves as a unified config for projects
//! It outputs pretty logs to the console stdout and stderr,
//! but also exports traces to a collector
//!

use pprof::ProfilerGuard;
use std::fmt::Debug;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

use crate::server_connection::instance_update_sender::export_instance_update;
use crate::subscriber::{ExportDataGetter, TracerTracingSubscriber};
use api_structs::instance::connect::RegistrationResponse;
use api_structs::instance::update::InstanceSnapshot;
pub use api_structs::{Env, InstanceId, ServiceId, Severity};
pub use print_debugging::print_if_dbg;

mod print_debugging;
mod server_connection;
mod subscriber;

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
    pub log_stdout: bool,
    pub log_stdout_json: bool,
}

impl TracerConfig {
    pub fn new(service_id: ServiceId, collector_url: String) -> TracerConfig {
        TracerConfig {
            collector_url,
            export_timeout: Duration::from_secs(10),
            duration_between_exports: Duration::from_secs(5),
            min_duration_between_profile_exports: Duration::from_secs(60),
            log_stdout: false,
            service_id,
            log_stdout_json: false,
        }
    }
    pub fn with_export_timeout(mut self, duration: Duration) {
        self.export_timeout = duration
    }
    pub fn with_sleep_between_exports(mut self, duration: Duration) {
        assert!(
            duration.as_secs() >= 2,
            "Sleep between exports needs to be at least 2s to not flood collector"
        );
        self.duration_between_exports = duration
    }
}

pub struct TracerHandle {
    pub thread_handle: std::thread::JoinHandle<()>,
    pub export_now_requester: ExportNowRequester,
}

pub async fn setup_tracer_client_in_background_or_panic(config: TracerConfig) -> TracerHandle {
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
    TracerHandle {
        thread_handle,
        export_now_requester,
    }
}

struct TracerTasks {
    sse_task: tokio::task::JoinHandle<()>,
    trace_export_task: tokio::task::JoinHandle<()>,
    export_now_request_sender: ExportNowRequester,
}

impl TracerTasks {
    pub async fn wait_or_panic(self) {
        let _res = futures::try_join!(self.sse_task, self.trace_export_task).unwrap();
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
) -> RegistrationResponse {
    let context = "registration_loop";
    loop {
        match server_connection::instance_registration::register_instance(
            &reqwest_client,
            collector_url,
            Duration::from_secs(10),
        )
        .await
        {
            Ok(registration_response) => return registration_response,
            Err(err) => {
                let sleep_seconds = 60;
                print_if_dbg(
                    context,
                    format!("Registration failure: {} - sleeping {sleep_seconds}s", err),
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
    let registration_response = registration_loop(&reqwest_client, &config.collector_url).await;
    let (export_now_request_receiver, export_now_request_sender) = ExportNowRequester::new();
    let cpu_profiler_guard = start_cpu_profiler();

    let tracer_filter = EnvFilter::builder()
        .parse(registration_response.log_filter)
        .expect("initial filters to be valid");
    let (reloadable_tracer_filter, reload_tracer_handle) =
        tracing_subscriber::reload::Layer::new(tracer_filter);

    let tracer_tracing_subscriber = TracerTracingSubscriber::new();
    let export_data_getter = tracer_tracing_subscriber.export_data_getter_handle();

    let registry = Registry::default()
        .with(reloadable_tracer_filter)
        .with(tracer_tracing_subscriber)
        .with(tracing_subscriber::fmt::layer());
    tracing::subscriber::set_global_default(registry).expect("no other global subscriber to exist");
    let instance_id = InstanceId {
        service_id: config.service_id.clone(),
        instance_id: uuid::Uuid::new_v4(),
    };
    let sse_task = tokio::task::spawn_local(
        server_connection::server_sent_events::continuously_handle_server_sent_events(
            instance_id.clone(),
            config.collector_url.clone(),
            reload_tracer_handle.clone(),
        ),
    );

    let trace_export_task = tokio::task::spawn_local(trace_export_loop(
        reqwest_client,
        config,
        cpu_profiler_guard,
        export_now_request_receiver,
        reload_tracer_handle,
        export_data_getter,
        instance_id,
    ));
    install_global_export_traces_on_panic_hook(export_now_request_sender.clone());
    TracerTasks {
        sse_task,
        trace_export_task,
        export_now_request_sender,
    }
}

async fn trace_export_loop(
    client: reqwest::Client,
    config: TracerConfig,
    profiler_guard: ProfilerGuard<'static>,
    mut flush_request_receiver: Receiver<FlushRequest>,
    reload_tracer_handle: tracing_subscriber::reload::Handle<EnvFilter, Registry>,
    export_data_getter: ExportDataGetter,
    instance_id: InstanceId,
) {
    let context = "trace_export_task";
    let min_wait_duration_between_profile_exports = config.min_duration_between_profile_exports;
    let mut time_last_profile_export = std::time::Instant::now();
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
        let current_filters = reload_tracer_handle
            .with_current(|c| c.to_string())
            .expect("subscriber to exist");
        print_if_dbg(context, "Checking for new events");

        let should_export_profile = (time_last_profile_export.elapsed()
            > min_wait_duration_between_profile_exports)
            || flush_request.is_some();
        let profile_data = if should_export_profile {
            time_last_profile_export = std::time::Instant::now();
            let mut profile_data = Vec::new();
            profiler_guard
                .report()
                .build()
                .expect("profile creation to work")
                .flamegraph(&mut profile_data)
                .expect("profile flamegraph generation to work");
            Some(profile_data)
        } else {
            None
        };
        let traces_and_orphan_events = export_data_getter.get_data_ready_to_export();
        let export_data = InstanceSnapshot {
            instance_id: instance_id.instance_id,
            orphan_events: traces_and_orphan_events.orphan_events,
            trace_snapshots: traces_and_orphan_events.traces,
            export_buffer_size_bytes: traces_and_orphan_events.export_buffer_size_bytes,
            log_filter: current_filters,
            cpu_profile: profile_data,
        };
        print_if_dbg(context, format!("Export data: {:#?}", export_data));
        let export_data_json =
            serde_json::to_string(&export_data).expect("export data to be serializable");
        loop {
            print_if_dbg(context, "attempting export");
            match export_instance_update(
                &client,
                &config.collector_url,
                &export_data_json,
                config.export_timeout,
            )
            .await
            {
                Ok(()) => break,
                Err(err) => {
                    let err = tracked_error::error_chain_to_pretty_formatted(err);
                    println!("{context} - {err}");
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

#[cfg(test)]
mod test {
    pub fn enable_logging_for_tests() {
        tracing_subscriber::fmt::try_init().ok();
        std::env::set_var("TRACER_DEBUG", "true");
    }
}

fn install_global_export_traces_on_panic_hook(export_now_handle: ExportNowRequester) {
    let current = std::panic::take_hook();
    println!("Installing panic hook");
    std::panic::set_hook(Box::new(move |panic_info| {
        println!("Running panic hook, trying to export creating and exporting panic span.");
        // Make sure we signal that we panic
        let panic_span = tracing::info_span!("program panicked", is_panic = true);
        panic_span.in_scope(|| {
            let bt = std::backtrace::Backtrace::force_capture();
            let panic_info: String = panic_info.to_string().chars().take(30_000).collect();
            let bt: String = bt.to_string().chars().take(30_000).collect();
            tracing::error!("Code panicked: Panic info: {}.", panic_info);
            tracing::error!("Backtrace:\n{bt}.");
        });
        if let Err(e) = export_now_handle.try_export_dont_wait_result() {
            println!("{:?}", e);
        }
        let wait_secs = 3;
        println!("Waiting {wait_secs} seconds so export hopefully finishes");
        std::thread::sleep(Duration::from_secs(wait_secs));
        current(panic_info)
    }));
}

// convenience helper so consumers don't need to import tracing_subscriber
pub fn init_stdout_tracing_for_tests(rust_log: &str) {
    std::env::set_var("RUST_LOG", rust_log);
    tracing_subscriber::fmt::try_init().ok();
}
