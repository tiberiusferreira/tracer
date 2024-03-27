//! This serves as a unified config for projects
//! It outputs pretty logs to the console stdout and stderr,
//! but also exports traces to a collector
//!

use pprof::ProfilerGuard;
use std::collections::HashMap;
use std::fmt::Debug;
use std::time::Duration;

use rand::random;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

use api_structs::instance::update::{ExportedServiceTraceData, NewOrphanEvent, TraceState};
pub use api_structs::{Env, InstanceId, ServiceId, Severity};
pub use print_debugging::print_if_dbg;
pub use subscriber::TRACER_RENAME_SPAN_TO_KEY;
use tracing::Level;

use crate::server_connection::instance_update_sender::export_instance_update;
use crate::subscriber::{ExporterStateHandle, SamplerHandle, TracerTracingSubscriber};

mod print_debugging;
mod server_connection;
mod subscriber;

pub const UPDATE_ENDPOINT: &str = "/api/instance/update";
pub const SSE_CONNECT_ENDPOINT: &str = "/api/instance/connect";

#[derive(Debug, Clone)]
pub struct TracerConfig {
    /// Where to send data to, should not contain a trailing /
    pub collector_url: String,
    pub service_id: ServiceId,
    /// The initial filters. Initial because these can be changed during runtime and this field does not reflect that
    /// change
    pub initial_filters: String,
    /// How long to wait for when exporting data before timing out
    pub export_timeout: Duration,
    /// How long to wait between exports. A short duration will flood the collector and a long one will cause the
    /// export buffers to fill up. Stats are also exported on this schedule.
    pub wait_duration_between_exports: Duration,
    pub min_wait_duration_between_profile_exports: Duration,
    pub log_stdout: bool,
    pub log_stdout_json: bool,
}

impl TracerConfig {
    pub fn new(service_id: ServiceId, collector_url: String) -> TracerConfig {
        TracerConfig {
            collector_url,
            initial_filters: std::env::var("RUST_LOG").unwrap_or_else(|_| {
                println!("RUST_LOG not found, defaulting to info");
                "info".to_string()
            }),
            export_timeout: Duration::from_secs(10),
            wait_duration_between_exports: Duration::from_secs(5),
            min_wait_duration_between_profile_exports: Duration::from_secs(60),
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
        self.wait_duration_between_exports = duration
    }
}

pub async fn setup_tracer_client_or_panic(config: TracerConfig) -> ExportNowRequester {
    println!("Starting up using: {config:#?}");
    // we start a new thread and runtime so it can still get data and debug issues involving the main program async
    // runtime starved from CPU time
    let (s, r) = tokio::sync::oneshot::channel();
    let _thread_handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("tracer_thread")
            .build()
            .expect("runtime to be able to start");

        runtime.block_on(async {
            // we use a localset so tasks dont have to implement Send
            tokio::task::LocalSet::new()
                .run_until(async {
                    let export_flusher_handle = setup_tracer_client_or_panic_impl(config).await;
                    s.send(export_flusher_handle.export_now_request_sender.clone())
                        .unwrap();
                    export_flusher_handle.wait_or_panic().await;
                })
                .await;
        });
    });
    r.await.unwrap()
}

struct TracerTasks {
    sse_task: JoinHandle<()>,
    trace_export_task: JoinHandle<()>,
    export_now_request_sender: ExportNowRequester,
}

impl TracerTasks {
    pub async fn wait_or_panic(self) {
        let _res = futures::try_join!(self.sse_task, self.trace_export_task).unwrap();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportDataContainers {
    data: ExportedServiceTraceData,
}

impl ExportDataContainers {
    pub fn new(
        instance_id: InstanceId,
        rust_log: String,
        orphan_events: Vec<NewOrphanEvent>,
        traces_state: HashMap<u64, TraceState>,
        profile_data: Option<Vec<u8>>,
    ) -> Self {
        Self {
            data: ExportedServiceTraceData {
                instance_id,
                orphan_events,
                traces_state,
                rust_log,
                profile_data,
            },
        }
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

async fn setup_tracer_client_or_panic_impl(config: TracerConfig) -> TracerTasks {
    let (export_now_request_receiver, export_now_request_sender) = ExportNowRequester::new();
    let cpu_profiler_guard = start_cpu_profiler();

    let tracer_filter = EnvFilter::builder()
        .parse(&config.initial_filters)
        .expect("initial filters to be valid");
    let (reloadable_tracer_filter, reload_tracer_handle) =
        tracing_subscriber::reload::Layer::new(tracer_filter);

    let tracer_tracing_subscriber = TracerTracingSubscriber::new();
    let tracer_sampler = tracer_tracing_subscriber.get_sampler_handle();
    let tracer_export_state = tracer_tracing_subscriber.get_sampler_state_handle();

    let registry = Registry::default()
        .with(reloadable_tracer_filter)
        .with(tracer_tracing_subscriber)
        .with(tracing_subscriber::fmt::layer());
    tracing::subscriber::set_global_default(registry).expect("no other global subscriber to exist");
    let instance_id = InstanceId {
        service_id: config.service_id.clone(),
        instance_id: random::<i64>(),
    };
    let sse_task = tokio::task::spawn_local(
        server_connection::server_sent_events::continuously_handle_server_sent_events(
            instance_id.clone(),
            config.collector_url.clone(),
            reload_tracer_handle.clone(),
        ),
    );

    let trace_export_task = tokio::task::spawn_local(trace_export_loop(
        config,
        cpu_profiler_guard,
        export_now_request_receiver,
        reload_tracer_handle,
        tracer_sampler,
        tracer_export_state,
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
    config: TracerConfig,
    profiler_guard: ProfilerGuard<'static>,
    mut flush_request_receiver: Receiver<FlushRequest>,
    reload_tracer_handle: tracing_subscriber::reload::Handle<EnvFilter, Registry>,
    tracer_sampler: SamplerHandle,
    tracer_export_state: ExporterStateHandle,
    instance_id: InstanceId,
) {
    let min_wait_duration_between_profile_exports =
        config.min_wait_duration_between_profile_exports;
    let client = reqwest::ClientBuilder::new()
        .build()
        .expect("reqwest client to be able to be created");
    let context = "trace_export_task";
    let profiler_guard = profiler_guard;
    let mut time_last_profile_export = std::time::Instant::now();
    loop {
        let period_time_secs = config.wait_duration_between_exports;
        print_if_dbg(
            context,
            format!(
                "Sleeping {}s or until flush request",
                period_time_secs.as_secs()
            ),
        );
        let flush_request = tokio::select! {
            _ = tokio::time::sleep(period_time_secs) => {
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
                .expect("profile writing to work");
            Some(profile_data)
        } else {
            None
        };
        let traces_and_orphan_events = tracer_export_state.get_export_data();
        let export_data = ExportDataContainers::new(
            instance_id.clone(),
            current_filters,
            traces_and_orphan_events.orphan_events,
            traces_and_orphan_events.traces,
            profile_data,
        );
        print_if_dbg(context, format!("Export data: {:#?}", export_data));
        let export_data_json =
            serde_json::to_string(&export_data.data).expect("export data to be serializable");
        let export_update_result = match export_instance_update(
            &client,
            &config.collector_url,
            &export_data_json,
            config.export_timeout,
        )
        .await
        {
            Ok(new_sampling) => {
                tracer_sampler.set_new(new_sampling);
                Ok(())
            }
            Err(err) => {
                let err = backtraced_error::error_chain_to_pretty_formatted(err);
                println!("{context} - {}", err);
                Err(err)
            }
        };
        flush_request.map(|f| match f.respond_to.send(export_update_result) {
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
