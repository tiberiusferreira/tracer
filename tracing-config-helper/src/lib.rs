//! This serves as an unified config for projects  
//! It outputs pretty logs to the console stdout and stderr,
//! but also exports traces to a collector
//!

use api_structs::exporter::trace_exporting::{
    ExportedServiceTraceData, SpanEventCount, TraceFragment,
};
use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::io::Read;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};
mod sampling;
mod server_connection;
mod subscriber;

pub fn print_if_dbg<StmType: AsRef<str>>(context: &'static str, debug_statement: StmType) {
    if debugging() {
        println!("{} - {}", context, debug_statement.as_ref());
    }
}

fn debugging() -> bool {
    static DEBUG: OnceLock<bool> = OnceLock::new();
    *DEBUG.get_or_init(|| {
        let debug = std::env::var("TRACER_DEBUG")
            .unwrap_or("0".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        debug
    })
}

/// Why a custom non-otel subscriber/exporter?
/// 1. Grouping of spans into a trace at service level
///  -  Dropped as a goal because:
///     - We wanted to be able to stream span and events to the collector to get an experience
///    closer to the console log, of immediate feedback.
///     - We also want to be able to see "in progress" traces to debug "endless" traces
/// 2. Alerting or sending "standalone" events
/// 3. Tail sampling, possible due to 1
///  - Postponed, needs thought about really needing it.
///   With Span+Event per Trace rate limiting we should get good representative data for all trace, most of the
///   time
/// 4. Send service health data
/// 4.1 Health check, heart beat
/// 5. Limit on Span+Event count per trace
/// 5.1 When hit stop recording new events or spans for that trace
/// 6. Limit on total Span+Events per minute per TopLevelSpan
/// Change log level for full trace
pub struct TracerTracingSubscriber {
    sampler: Arc<parking_lot::RwLock<TracerSampler>>,
    subscriber_event_sender: Sender<SubscriberEvent>,
}

use crate::sampling::{Sampler, TracerSampler};

use tokio::sync::mpsc::{Receiver, Sender};

use api_structs::exporter::status::{SamplerLimits, TracerStatus};
use api_structs::{Env, Severity};
use rand::random;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct TracerConfig {
    /// Where to send data to, should not contains a trailing /
    pub collector_url: String,
    /// Which kind of environment its running at
    pub env: Env,
    /// The name to advertise this service as, should normally be the binary name which can be obtained from using
    /// env!("CARGO_BIN_NAME")
    pub service_name: String,
    /// The initial filters. Initial because these can be changed during runtime and this field does not reflect that
    /// change
    pub initial_filters: String,
    /// How long to wait for when exporting data before timing out
    pub export_timeout: Duration,
    /// How long to wait between exports. A short duration will flood the collector and a long one will cause the
    /// export buffers to fill up. Stats are also exported on this schedule.
    pub wait_duration_between_exports: Duration,
    pub sampler_limits: SamplerLimits,
    /// Maximum number of span plus events to keep in memory at a given time
    pub spe_buffer_capacity: u64,
}

impl TracerConfig {
    pub fn new(env: Env, service_name: String, collector_url: String) -> TracerConfig {
        TracerConfig {
            collector_url,
            env,
            service_name,
            initial_filters: std::env::var("RUST_LOG").unwrap_or_else(|_| {
                println!("RUST_LOG not found, defaulting to info");
                "info".to_string()
            }),
            export_timeout: Duration::from_secs(10),
            wait_duration_between_exports: Duration::from_secs(3),
            sampler_limits: SamplerLimits {
                trace_spe_per_minute_per_trace_limit: 1000,
                extra_spe_per_minute_limit_for_existing_traces: 5000,
                logs_per_minute_limit: 1000,
            },
            spe_buffer_capacity: 10_000,
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
    pub fn with_limits(mut self, sampler_limits: SamplerLimits) {
        self.sampler_limits = sampler_limits
    }
}

pub async fn setup_tracer_client_or_panic(config: TracerConfig) -> FlushRequester {
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
                    s.send(export_flusher_handle.flush_request_sender.clone())
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
    flush_request_sender: FlushRequester,
}

impl TracerTasks {
    pub async fn wait_or_panic(self) {
        let _res = futures::try_join!(self.sse_task, self.trace_export_task).unwrap();
    }
}

#[derive(Debug, Clone)]
pub enum SubscriberEvent {
    NewSpan(NewSpan),
    NewSpanEvent(NewSpanEvent),
    ClosedSpan(api_structs::exporter::trace_exporting::ClosedSpan),
    NewOrphanEvent(api_structs::exporter::trace_exporting::NewOrphanEvent),
    SpanEventCountUpdate {
        trace_id: u64,
        trace_name: &'static str,
        trace_timestamp: u64,
        spe_count: SpanEventCount,
    },
}

#[derive(Debug, Clone)]
pub struct NewSpan {
    pub trace_id: u64,
    pub trace_name: &'static str,
    pub spe_count: SpanEventCount,
    pub trace_timestamp: u64,
    pub id: u64,
    pub timestamp: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub key_vals: HashMap<String, String>,
}
#[derive(Debug, Clone)]
pub struct NewSpanEvent {
    pub trace_id: u64,
    pub trace_name: &'static str,
    pub spe_count: SpanEventCount,
    pub trace_timestamp: u64,
    pub span_id: u64,
    pub message: Option<String>,
    pub timestamp: u64,
    pub level: Severity,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportDataContainers {
    data: ExportedServiceTraceData,
}

impl ExportDataContainers {
    pub fn new(
        service_id: i64,
        service_name: String,
        env: Env,
        filters: String,
        tracer_stats: TracerStatus,
        active_trace_fragments: HashMap<u64, TraceFragment>,
    ) -> Self {
        Self {
            data: ExportedServiceTraceData {
                service_id,
                service_name,
                env,
                active_trace_fragments,
                closed_spans: vec![],
                orphan_events: vec![],
                rust_log: filters,
                tracer_stats,
            },
        }
    }

    pub fn add_event(&mut self, event: SubscriberEvent) {
        match event {
            SubscriberEvent::NewSpan(span) => {
                let trace = self
                    .data
                    .active_trace_fragments
                    .entry(span.trace_id)
                    .or_insert(TraceFragment {
                        trace_id: span.trace_id,
                        trace_name: span.trace_name.to_string(),
                        trace_timestamp: span.trace_timestamp,
                        spe_count: span.spe_count.clone(),
                        new_spans: vec![],
                        new_events: vec![],
                    });
                trace
                    .new_spans
                    .push(api_structs::exporter::trace_exporting::NewSpan {
                        id: span.id,
                        timestamp: span.timestamp,
                        duration: None,
                        parent_id: span.parent_id,
                        name: span.name,
                        key_vals: span.key_vals,
                    });
                trace.spe_count = span.spe_count;
            }
            SubscriberEvent::NewSpanEvent(span_event) => {
                let trace = self
                    .data
                    .active_trace_fragments
                    .entry(span_event.trace_id)
                    .or_insert(TraceFragment {
                        trace_id: span_event.trace_id,
                        trace_name: span_event.trace_name.to_string(),
                        trace_timestamp: span_event.trace_timestamp,
                        spe_count: span_event.spe_count.clone(),
                        new_spans: vec![],
                        new_events: vec![],
                    });
                trace
                    .new_events
                    .push(api_structs::exporter::trace_exporting::NewSpanEvent {
                        span_id: span_event.span_id,
                        timestamp: span_event.timestamp,
                        message: span_event.message,
                        key_vals: span_event.key_vals,
                        level: span_event.level,
                    });
                trace.spe_count = span_event.spe_count;
            }
            SubscriberEvent::ClosedSpan(closed) => {
                match self.data.active_trace_fragments.get_mut(&closed.trace_id) {
                    None => {
                        self.data.closed_spans.push(closed);
                    }
                    Some(trace) => {
                        match trace.new_spans.iter_mut().find(|s| s.id == closed.span_id) {
                            None => {
                                self.data.closed_spans.push(closed);
                            }
                            Some(span) => span.duration = Some(closed.duration),
                        }
                    }
                }
            }
            SubscriberEvent::NewOrphanEvent(orphan) => {
                self.data.orphan_events.push(orphan);
            }
            SubscriberEvent::SpanEventCountUpdate {
                trace_id,
                trace_name,
                trace_timestamp,
                spe_count,
            } => {
                let trace =
                    self.data
                        .active_trace_fragments
                        .entry(trace_id)
                        .or_insert(TraceFragment {
                            trace_id,
                            trace_name: trace_name.to_string(),
                            trace_timestamp,
                            spe_count: spe_count.clone(),
                            new_spans: vec![],
                            new_events: vec![],
                        });
                trace.spe_count = spe_count;
            }
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

#[derive(Debug, Clone)]
pub struct FlushRequester {
    sender_channel: Sender<FlushRequest>,
}

impl FlushRequester {
    fn new() -> (Receiver<FlushRequest>, FlushRequester) {
        let (sender, receiver) = tokio::sync::mpsc::channel::<FlushRequest>(1);
        (
            receiver,
            Self {
                sender_channel: sender,
            },
        )
    }
    pub async fn flush(&self, timeout: Duration) -> Result<(), String> {
        let (receiver, request) = FlushRequest::new();
        self.sender_channel.try_send(request).map_err(|_e| {
            "Error sending request receiving channel is closed or blocked".to_string()
        })?;
        tokio::time::timeout(timeout, receiver)
            .await
            .map_err(|_e| "Timeout waiting".to_string())?
            .map_err(|_e| "Error getting response".to_string())?
    }
}

async fn setup_tracer_client_or_panic_impl(config: TracerConfig) -> TracerTasks {
    println!("Initializing Tracer using:\n{:#?}", config);
    let (mut flush_request_receiver, flush_request_sender) = FlushRequester::new();
    let spe_buffer_len = usize::try_from(config.spe_buffer_capacity).expect("u32 to fit usize");
    let filter = EnvFilter::builder()
        .parse(&config.initial_filters)
        .expect("initial filters to be valid");

    let (reloadable_filter, reload_handle) = tracing_subscriber::reload::Layer::new(filter);
    let (subscriber_event_sender, mut subscriber_event_receiver) =
        tokio::sync::mpsc::channel::<SubscriberEvent>(spe_buffer_len);
    let tracer =
        TracerTracingSubscriber::new(config.sampler_limits.clone(), subscriber_event_sender);
    let tracer_sampler = Arc::clone(&tracer.sampler);

    let trace_with_filter = tracer.with_filter(reloadable_filter);
    let registry = Registry::default().with(trace_with_filter);
    tracing::subscriber::set_global_default(registry).expect("no other global subscriber to exist");
    let service_id = random::<i64>();
    let sse_task =
        tokio::task::spawn_local(server_connection::continuously_handle_server_sent_events(
            config.collector_url.clone(),
            reload_handle.clone(),
            service_id,
        ));

    let trace_export_task = tokio::task::spawn_local(async move {
        let spe_buffer_capacity = config.spe_buffer_capacity;
        let env = config.env;
        let client = reqwest::Client::new();
        let context = "trace_export_task";
        let mut active_traces = HashMap::new();
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
                    print_if_dbg(context, format!("Slept"));
                    None
                },
                received_val = flush_request_receiver.recv() => {
                    match received_val{
                        Some(flush_request) => {
                            print_if_dbg(context, format!("Got flush request"));
                            Some(flush_request)
                        }
                        None => {
                            print_if_dbg(context, format!("Flush request channel is closed, sleeping"));
                            tokio::time::sleep(period_time_secs).await;
                            None
                        }
                    }
                },
            };
            let current_filters = reload_handle
                .with_current(|c| c.to_string())
                .expect("subscriber to exist");
            print_if_dbg(context, "Checking for new events");
            // we need this because events come in reverse order
            let mut events = VecDeque::new();
            while let Ok(event) = subscriber_event_receiver.try_recv() {
                events.push_back(event);
            }
            let mut tracer_stats = tracer_sampler.read().get_tracer_stats();
            tracer_stats.spe_buffer_capacity = spe_buffer_capacity;
            tracer_stats.spe_buffer_usage = events.len() as u64;
            let mut export_data = ExportDataContainers::new(
                service_id,
                config.service_name.to_string(),
                env,
                current_filters,
                tracer_stats,
                active_traces.clone(),
            );

            print_if_dbg(context, format!("New events count: {}", events.len()));
            print_if_dbg(context, format!("Event List: {:#?}", events));
            for e in events {
                export_data.add_event(e);
            }
            print_if_dbg(context, format!("Export data: {:#?}", export_data));
            let export_data_json =
                serde_json::to_string(&export_data.data).expect("export data to be serializable");
            active_traces = export_data
                .data
                .active_trace_fragments
                .into_iter()
                .filter_map(|(id, mut frag)| {
                    let root_closed = frag
                        .new_spans
                        .iter()
                        .any(|span| span.id == id && span.duration.is_some());
                    if root_closed {
                        return None;
                    }
                    let trace_old_root_closed = export_data
                        .data
                        .closed_spans
                        .iter()
                        .any(|closed| closed.trace_id == id && closed.span_id == id);
                    if trace_old_root_closed {
                        return None;
                    }
                    frag.new_spans.clear();
                    frag.new_events.clear();
                    Some((id, frag))
                })
                .collect();
            print_if_dbg(
                context,
                format!(
                    "Response before compression: {} bytes",
                    export_data_json.len()
                ),
            );

            let lg_window_size = 21;
            let quality = 4;
            let mut input = brotli::CompressorReader::new(
                export_data_json.as_bytes(),
                4096,
                quality as u32,
                lg_window_size as u32,
            );
            let mut export_data_json: Vec<u8> = Vec::with_capacity(100 * 1000);
            input.read_to_end(&mut export_data_json).unwrap();
            print_if_dbg(
                context,
                format!(
                    "Response after compression: {} bytes",
                    export_data_json.len()
                ),
            );

            let send_result = match client
                .post(format!("{}/collector/trace_data", config.collector_url))
                .body(export_data_json)
                .header("Content-Type", "application/json")
                .timeout(config.export_timeout)
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    print_if_dbg(
                        context,
                        format!("Sent events and got success response: {response:#?}"),
                    );
                    Ok(())
                }
                Ok(response) => {
                    let status = response.status();
                    let err = format!(
                        "Sent events, but got fail response: {status}.\nResponse:{response:#?}"
                    );
                    print_if_dbg(context, &err);
                    Err(err)
                }
                Err(e) => {
                    let err = format!("Error sending events: {e:#?}");
                    print_if_dbg(context, format!("Error sending events: {e:#?}"));
                    Err(err)
                }
            };
            if let Some(flush_request) = flush_request {
                match flush_request.respond_to.send(send_result) {
                    Ok(_) => {
                        println!("Responded to flush request");
                    }
                    Err(_e) => {
                        println!("Error responding to flush request");
                    }
                }
            }
        }
    });
    TracerTasks {
        sse_task,
        trace_export_task,
        flush_request_sender,
    }
}
