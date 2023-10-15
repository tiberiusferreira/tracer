//! This serves as an unified config for projects  
//! It outputs pretty logs to the console stdout and stderr,
//! but also exports traces to a collector
//!

use api_structs::exporter::{
    ClosedSpan, ExportedServiceTraceData, NewOrphanEvent, NewSpan, NewSpanEvent, SamplerLimits,
    Severity, SubscriberEvent, TracerFilters,
};
use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tracing::field::{Field, Visit};
use tracing::span::Record;
use tracing::subscriber::{self};
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::filter::ParseError;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::{LookupSpan, SpanRef};
use tracing_subscriber::{EnvFilter, Layer, Registry};
mod old;
mod sampling;
mod test;
pub use old::*;
mod server_connection;

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

impl TracerTracingSubscriber {
    fn new(
        sampler_limits: SamplerLimits,
        subscriber_event_sender: Sender<SubscriberEvent>,
    ) -> Self {
        let sampler = Arc::new(parking_lot::RwLock::new(TracerSampler::new(sampler_limits)));
        let tracer = Self {
            sampler,
            subscriber_event_sender,
        };
        tracer
    }

    fn span_root<'a, S: Subscriber + for<'b> LookupSpan<'b>>(
        id: Id,
        ctx: &'a Context<S>,
    ) -> Option<SpanRef<'a, S>> {
        let root = ctx.span(&id)?.scope().from_root().next()?;
        Some(root)
    }
    fn trace_was_dropped<'a, S: Subscriber + for<'b> LookupSpan<'b>>(
        span_id: Id,
        ctx: &'a Context<S>,
    ) -> bool {
        let root_span = Self::span_root(span_id, &ctx).expect("root to exist even if itself");
        let root_extensions = root_span.extensions();
        let tracer_root_span_data: &TracerRootSpanData = root_extensions
            .get()
            .expect("root span to have TracerRootSpanData");
        tracer_root_span_data.dropped
    }
    fn send_subscriber_event_to_export(&self, subscriber_event: SubscriberEvent) {
        let context = "send_subscriber_event_to_export";
        match self
            .subscriber_event_sender
            .try_send(subscriber_event.clone())
        {
            Ok(_) => {
                print_if_dbg(context, format!("Send event {:#?}", subscriber_event));
            }
            Err(_e) => {
                print_if_dbg(
                    context,
                    format!("Send failed for event {:#?}", subscriber_event),
                );
                self.sampler.write().register_soe_dropped_on_export();
            }
        }
    }
}

struct MyV {
    message: Option<String>,
}
impl Visit for MyV {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        }
    }
}

use crate::sampling::{Sampler, TracerSampler};

struct TracerSpanData {
    first_entered_at: std::time::Instant,
}
struct TracerRootSpanData {
    dropped: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: u64,
    pub level: Severity,
    pub key_vals: HashMap<String, String>,
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for TracerTracingSubscriber {
    fn on_record(&self, _span: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {
        let context = "on_record";
        print_if_dbg(context, "on record");
    }
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let context = "on_event";
        let span = ctx.event_span(event);
        let event_data = {
            let mut my_v = MyV { message: None };
            event.record(&mut my_v);
            let name = if let Some(msg) = my_v.message {
                print_if_dbg(context, format!("New event: {msg}"));
                msg
            } else {
                print_if_dbg(context, format!("New empty event, dropping it"));
                println!("ALERT: Empty events are not supported yet!");
                return;
            };
            let level = match event.metadata().level() {
                &tracing::metadata::Level::TRACE => Severity::Trace,
                &tracing::metadata::Level::DEBUG => Severity::Debug,
                &tracing::metadata::Level::INFO => Severity::Info,
                &tracing::metadata::Level::WARN => Severity::Warn,
                &tracing::metadata::Level::ERROR => Severity::Error,
            };
            SpanEvent {
                name,
                timestamp: api_structs::time_conversion::now_nanos_u64(),
                level,
                key_vals: Default::default(),
            }
        };

        let Some(span) = span else {
            print_if_dbg(context, format!("Event is orphan"));
            let mut w_sampler = self.sampler.write();
            let new_orphan_event_allowed = w_sampler.allow_new_orphan_event();
            drop(w_sampler);
            return if new_orphan_event_allowed {
                print_if_dbg(context, format!("Allowed by sampler, sending to exporter"));
                self.send_subscriber_event_to_export(SubscriberEvent::NewOrphanEvent(
                    NewOrphanEvent {
                        name: event_data.name,
                        timestamp: event_data.timestamp,
                        level: event_data.level,
                        key_vals: event_data.key_vals,
                    },
                ));
            } else {
                print_if_dbg(context, format!("Not Allowed by sampler, dropping"));
            };
        };
        if Self::trace_was_dropped(span.id(), &ctx) {
            print_if_dbg(
                context,
                format!("Event belongs to trace previously dropped, dropping event."),
            );
            return;
        }
        let mut w_sampler = self.sampler.write();
        let root = Self::span_root(span.id(), &ctx).expect("root span to exist");
        let new_event_allowed = w_sampler.allow_new_event(root.name());
        drop(w_sampler);
        if new_event_allowed {
            print_if_dbg(context, format!("Allowed by sampler, sending to exporter."));
            self.send_subscriber_event_to_export(SubscriberEvent::NewSpanEvent(NewSpanEvent {
                trace_id: root.id().into_non_zero_u64(),
                span_id: span.id().into_non_zero_u64(),
                name: event_data.name,
                timestamp: event_data.timestamp,
                level: event_data.level,
                key_vals: event_data.key_vals,
            }));
        }
    }
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let context = "on_enter";
        let span = ctx.span(id).expect("entered span to exist!");
        let span_name = span.name();
        let mut extensions = span.extensions_mut();
        let tracer_span_data: Option<&mut TracerSpanData> = extensions.get_mut();
        if tracer_span_data.is_some() {
            print_if_dbg(context, format!("span {span_name} entered again"));
            return;
        } else {
            print_if_dbg(
                context,
                format!("span {span_name} entered for the first time"),
            );
            extensions.insert(TracerSpanData {
                first_entered_at: std::time::Instant::now(),
            });
        }
        drop(extensions);
        let root_span = Self::span_root(id.clone(), &ctx).expect("root span to exist");

        if root_span.id() == *id {
            print_if_dbg(context, format!("Span is root. Id: {}", id.into_u64()));
            let mut w_sampler = self.sampler.write();
            let new_trace_allowed = w_sampler.allow_new_trace(&root_span.name());
            drop(w_sampler);
            if new_trace_allowed {
                print_if_dbg(context, "Allowed by sampler, sending to exporter");
                self.send_subscriber_event_to_export(SubscriberEvent::NewSpan(NewSpan {
                    id: id.into_non_zero_u64(),
                    trace_id: root_span.id().into_non_zero_u64(),
                    name: span_name.to_string(),
                    parent_id: None,
                    timestamp: u64::try_from(
                        chrono::Utc::now()
                            .naive_utc()
                            .timestamp_nanos_opt()
                            .unwrap(),
                    )
                    .unwrap(),
                    key_vals: Default::default(),
                }));
                root_span
                    .extensions_mut()
                    .insert(TracerRootSpanData { dropped: false })
            } else {
                print_if_dbg(context, "Not Allowed by sampler");
                root_span
                    .extensions_mut()
                    .insert(TracerRootSpanData { dropped: true })
            }
        } else {
            // we are not root, check if current trace was dropped
            print_if_dbg(context, format!("New non-root span. Id: {}", id.into_u64()));
            if Self::trace_was_dropped(id.clone(), &ctx) {
                print_if_dbg(
                    context,
                    "Span belongs to previously dropped trace, dropping it.",
                );
                return;
            } else {
                print_if_dbg(context, "Span belongs to non-dropped trace");
                let mut w_sampler = self.sampler.write();
                let new_span_allowed = w_sampler.allow_new_span(root_span.name());
                drop(w_sampler);
                if new_span_allowed {
                    let parent_id = span.parent().expect("parent to exist if non-root").id();
                    print_if_dbg(context, "Allowed by sampler, sending to exporter");
                    self.send_subscriber_event_to_export(SubscriberEvent::NewSpan(NewSpan {
                        id: id.into_non_zero_u64(),
                        trace_id: root_span.id().into_non_zero_u64(),
                        name: span_name.to_string(),
                        parent_id: Some(parent_id.into_non_zero_u64()),
                        timestamp: u64::try_from(
                            chrono::Utc::now()
                                .naive_utc()
                                .timestamp_nanos_opt()
                                .unwrap(),
                        )
                        .unwrap(),
                        key_vals: Default::default(),
                    }));
                } else {
                    print_if_dbg(context, "Not Allowed by sampler");
                }
            }
        }
    }
    fn on_close(&self, span_id: Id, ctx: Context<'_, S>) {
        let context = "on_close";
        let span = ctx.span(&span_id).expect("span to exist if it got closed");
        if Self::trace_was_dropped(span.id(), &ctx) {
            print_if_dbg(
                context,
                "Span belongs to previously dropped trace, dropping it",
            );
            return;
        }
        let extensions = span.extensions();
        let tracer_span_data: &TracerSpanData = extensions
            .get()
            .expect("tracer span data to exist if span is closing");
        print_if_dbg(
            context,
            format!("Span {} closed. Sending to exporter", span_id.into_u64()),
        );
        self.send_subscriber_event_to_export(SubscriberEvent::ClosedSpan(ClosedSpan {
            id: span_id.into_non_zero_u64(),
            duration: api_structs::time_conversion::duration_u64_nanos_from_instant(
                tracer_span_data.first_entered_at,
            ),
        }));
    }
}

pub fn to_filter(filters: &TracerFilters) -> Result<EnvFilter, ParseError> {
    let as_str = filters.to_filter_str();
    EnvFilter::builder().parse(as_str)
}

use tokio::sync::mpsc::Sender;

use rand::random;
use tokio::task::JoinHandle;

#[test]
fn b() {
    let mut tracer_filters = TracerFilters {
        global: Severity::Info,
        per_crate: HashMap::new(),
        per_span: HashMap::new(),
    };
    tracer_filters
        .per_span
        .insert("SomeSpan".to_string(), Severity::Debug);
    tracer_filters
        .per_span
        .insert("A".to_string(), Severity::Trace);
    tracer_filters
        .per_span
        .insert("B".to_string(), Severity::Warn);
    let filter = EnvFilter::builder()
        .parse(tracer_filters.to_filter_str())
        .unwrap();
    println!("{}", tracer_filters.to_filter_str());
    println!("{}", filter.to_string());
    return;
}

pub struct TracerConfig {
    pub collector_url: String,
    pub env: String,
    pub service_name: String,
    pub filters: String,
    pub export_timeout: Duration,
    pub status_send_period: Duration,
    pub sampler_limits: SamplerLimits,
    pub maximum_spe_buffer: u32,
}

pub async fn setup_tracer_client_or_panic(config: TracerConfig) {
    // we start a new thread and runtime so it can still get data and debug issues involving the main program async
    // runtime starved from CPU time
    // let (s, r) = tokio::sync::oneshot::channel::<()>();
    let _thread_handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("tracer_thread")
            .build()
            .expect("runtime to be able to start");

        runtime.block_on(async {
            tokio::task::LocalSet::new()
                .run_until(async {
                    let export_flusher_handle = setup_tracer_client_or_panic_impl(config).await;
                    export_flusher_handle.wait_or_panic().await;
                    // s.send(export_flusher_handle)
                    //     .expect("receiver to exist and parent thread to not have panicked");
                    tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
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
async fn setup_tracer_client_or_panic_impl(config: TracerConfig) -> TracerTasks {
    let spe_buffer_len = usize::try_from(config.maximum_spe_buffer).expect("u32 to fit usize");
    println!("using filters: {}", config.filters);
    let filter = EnvFilter::builder().parse(&config.filters).unwrap();

    let (reloadable_filter, reload_handle) = tracing_subscriber::reload::Layer::new(filter);
    let (subscriber_event_sender, mut subscriber_event_receiver) =
        tokio::sync::mpsc::channel::<SubscriberEvent>(spe_buffer_len);
    let tracer =
        TracerTracingSubscriber::new(config.sampler_limits.clone(), subscriber_event_sender);
    let tracer_sampler = Arc::clone(&tracer.sampler);

    let trace_with_filter = tracer.with_filter(reloadable_filter);
    let registry = Registry::default().with(trace_with_filter);
    subscriber::set_global_default(registry).unwrap();
    let client_id = random::<i64>();
    let sse_task = tokio::task::spawn_local(
        server_connection::continuously_handle_server_sent_events(reload_handle.clone(), client_id),
    );

    let trace_export_task = tokio::task::spawn_local(async move {
        let client = reqwest::Client::new();
        let context = "trace_export_task";
        loop {
            let period_time_secs = 10;
            print_if_dbg(context, format!("Sleeping {}s", period_time_secs));
            tokio::time::sleep(Duration::from_secs(period_time_secs)).await;
            let mut events = VecDeque::new();
            print_if_dbg(context, "Checking for new events");
            while let Ok(event) = subscriber_event_receiver.try_recv() {
                events.push_back(event);
            }
            print_if_dbg(context, format!("New events count: {}", events.len()));
            print_if_dbg(context, format!("Event List: {:#?}", events));
            let current_filters = reload_handle
                .with_current(|c| c.to_string())
                .expect("subscriber to exist");
            let export_data = ExportedServiceTraceData {
                service_id: client_id,
                service_name: config.service_name.to_string(),
                events: events.into_iter().collect(),
                filters: current_filters,
                tracer_stats: tracer_sampler.read().get_tracer_stats(),
            };
            let export_data_json =
                serde_json::to_string(&export_data).expect("export data to be serializable");
            print_if_dbg(
                context,
                format!(
                    "Exporting: {} bytes containing {} events",
                    export_data_json.len(),
                    export_data.events.len()
                ),
            );
            match client
                .post("http://127.0.0.1:4200/collector/trace_data")
                .body(export_data_json)
                .header("Content-Type", "application/json")
                .timeout(Duration::from_secs(5))
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
