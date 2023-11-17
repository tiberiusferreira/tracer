//! This serves as an unified config for projects  
//! It outputs pretty logs to the console stdout and stderr,
//! but also exports traces to a collector
//!

use api_structs::exporter::{
    ExportedServiceTraceData, SamplerLimits, Severity, SpanEventCount, TraceFragment, TracerStats,
};
use std::collections::{HashMap, VecDeque};
use std::fmt::{Debug, Display, Formatter};
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

use tokio::sync::mpsc::Sender;

use rand::random;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct TracerConfig {
    /// Where to send data to, should not contains a trailing /
    pub collector_url: String,
    /// Which kind of environment its running at
    pub env: Env,
    /// The name to advertise this service as, should normally be the binary name
    pub service_name: String,
    /// The initial filters. Initial because these can be changed during runtime and this field does not reflect that
    /// change
    pub initial_filters: String,
    /// Timeout when exporting data
    pub export_timeout: Duration,
    /// How long to sleep between exports. A short duration will flood the collector and a long one will cause the
    /// export buffers to fill up. Stats are also exported on this schedule
    pub sleep_duration_between_exports: Duration,
    pub sampler_limits: SamplerLimits,
    /// Maximum number of span plus events to keep in memory at a given time
    pub maximum_span_plus_event_buffer: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum Env {
    Local,
    Dev,
    Stage,
    Prod,
}

impl Display for Env {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Env::Local => f.write_str("local"),
            Env::Dev => f.write_str("dev"),
            Env::Stage => f.write_str("stage"),
            Env::Prod => f.write_str("prod"),
        }
    }
}
impl TracerConfig {
    pub fn new(env: Env, service_name: String, collector_url: String) -> TracerConfig {
        TracerConfig {
            collector_url,
            env,
            service_name,
            initial_filters: std::env::var("RUST_LOG").unwrap_or_default(),
            export_timeout: Duration::from_secs(10),
            sleep_duration_between_exports: Duration::from_secs(10),
            sampler_limits: SamplerLimits {
                new_trace_span_plus_event_per_minute_per_trace_limit: 1000,
                existing_trace_span_plus_event_per_minute_limit: 5000,
                logs_per_minute_limit: 1000,
            },
            maximum_span_plus_event_buffer: 10_000,
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
        self.sleep_duration_between_exports = duration
    }
    pub fn with_limits(mut self, sampler_limits: SamplerLimits) {
        self.sampler_limits = sampler_limits
    }
}

pub async fn setup_tracer_client_or_panic(config: TracerConfig) {
    // we start a new thread and runtime so it can still get data and debug issues involving the main program async
    // runtime starved from CPU time
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
                    export_flusher_handle.wait_or_panic().await;
                })
                .await;
        });
    });
}

struct TracerTasks {
    sse_task: JoinHandle<()>,
    trace_export_task: JoinHandle<()>,
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
    ClosedSpan(api_structs::exporter::ClosedSpan),
    NewOrphanEvent(api_structs::exporter::NewOrphanEvent),
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
        filters: String,
        tracer_stats: TracerStats,
    ) -> Self {
        Self {
            data: ExportedServiceTraceData {
                service_id,
                service_name,
                total_span_count: 0,
                total_event_count: 0,
                trace_fragments: HashMap::new(),
                closed_spans: vec![],
                orphan_events: vec![],
                filters,
                tracer_stats,
            },
        }
    }

    pub fn add_event(&mut self, event: SubscriberEvent) {
        match event {
            SubscriberEvent::NewSpan(span) => {
                let trace =
                    self.data
                        .trace_fragments
                        .entry(span.trace_id)
                        .or_insert(TraceFragment {
                            trace_id: span.trace_id,
                            trace_name: span.trace_name.to_string(),
                            trace_timestamp: span.trace_timestamp,
                            spe_count: span.spe_count.clone(),
                            new_spans: vec![],
                            new_events: vec![],
                        });
                trace.new_spans.push(api_structs::exporter::NewSpan {
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
                    .trace_fragments
                    .entry(span_event.trace_id)
                    .or_insert(TraceFragment {
                        trace_id: span_event.trace_id,
                        trace_name: span_event.trace_name.to_string(),
                        trace_timestamp: span_event.trace_timestamp,
                        spe_count: span_event.spe_count.clone(),
                        new_spans: vec![],
                        new_events: vec![],
                    });
                trace.new_events.push(api_structs::exporter::NewSpanEvent {
                    span_id: span_event.span_id,
                    timestamp: span_event.timestamp,
                    message: span_event.message,
                    key_vals: span_event.key_vals,
                    level: span_event.level,
                });
                trace.spe_count = span_event.spe_count;
            }
            SubscriberEvent::ClosedSpan(closed) => {
                match self.data.trace_fragments.get_mut(&closed.trace_id) {
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
                let trace = self
                    .data
                    .trace_fragments
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

async fn setup_tracer_client_or_panic_impl(config: TracerConfig) -> TracerTasks {
    println!("Initializing Tracer using:\n{:#?}", config);
    let spe_buffer_len =
        usize::try_from(config.maximum_span_plus_event_buffer).expect("u32 to fit usize");
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
        let client = reqwest::Client::new();
        let context = "trace_export_task";
        loop {
            let period_time_secs = config.sleep_duration_between_exports;
            print_if_dbg(context, format!("Sleeping {}s", period_time_secs.as_secs()));
            tokio::time::sleep(period_time_secs).await;
            let current_filters = reload_handle
                .with_current(|c| c.to_string())
                .expect("subscriber to exist");
            let mut export_data = ExportDataContainers::new(
                service_id,
                config.service_name.to_string(),
                current_filters,
                tracer_sampler.read().get_tracer_stats(),
            );
            print_if_dbg(context, "Checking for new events");
            // we need this because events come in reverse order
            let mut events = VecDeque::new();
            while let Ok(event) = subscriber_event_receiver.try_recv() {
                events.push_back(event);
            }
            print_if_dbg(context, format!("New events count: {}", events.len()));
            print_if_dbg(context, format!("Event List: {:#?}", events));
            for e in events {
                export_data.add_event(e);
            }
            print_if_dbg(context, format!("Export data: {:#?}", export_data));
            let export_data_json =
                serde_json::to_string(&export_data.data).expect("export data to be serializable");
            print_if_dbg(
                context,
                format!("Exporting: {} bytes", export_data_json.len(),),
            );
            match client
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
                }
                Ok(response) => {
                    let status = response.status();
                    print_if_dbg(
                        context,
                        format!(
                            "Sent events, but got fail response: {status}.\nResponse:{response:#?}"
                        ),
                    );
                }
                Err(e) => {
                    print_if_dbg(context, format!("Error sending events: {e:#?}"));
                }
            }
        }
    });
    TracerTasks {
        sse_task,
        trace_export_task,
    }
}
