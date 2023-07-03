//! This serves as an unified config for projects  
//! It outputs pretty logs to the console stdout and stderr,
//! but also exports traces to a collector
//!

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use opentelemetry::sdk::trace;
use opentelemetry::sdk::trace::{BatchConfig, Tracer};
use opentelemetry_otlp::WithExportConfig;
use parking_lot::RwLock;

use api_structs::exporter::{SpanEvent, StatusData};
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Record};
use tracing::subscriber::{self};
use tracing::{debug, error, error_span, info, info_span, trace, warn, Event, Id, Subscriber};
use tracing_subscriber::filter::{Directive, ParseError};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::{EnvFilter, Layer, Registry};

mod export;
mod sampling;
mod storage;

/// This is a guard that will shutdown the OpenTelemetry exporter on drop.
/// TLDR; Keep this around in main to make sure it is dropped after the
/// program exits, due to regular causes our panic.
///
/// This is intended to be dropped after all the rest of the program
/// has finished running. This is also useful to be kept around
/// in programs intended to never exit (webservers for example)
/// because in case of a panic this gets dropped and the panic trace
/// is exported after the stack unwinding gets to main
#[derive(Debug)]
pub struct TraceShutdownGuard {
    tracer: Tracer,
}

impl Drop for TraceShutdownGuard {
    fn drop(&mut self) {
        match self.tracer.provider() {
            None => {
                panic!(
                    "TraceShutdownGuard dropped, but no tracer registered, this is likely a bug!"
                );
            }
            Some(provider) => {
                for export_res in provider.force_flush() {
                    if let Err(err) = export_res {
                        println!(
                            "Failed to export traces during TraceShutdownGuard drop, please, look into it: {:?}",
                            err
                        );
                    }
                }
            }
        }
        println!("Tracer is shutting down because the handle was dropped, traces will no longer be exported!");
        opentelemetry::global::shutdown_tracer_provider();
    }
}

/// Uses RUST_LOG, see https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html
/// on how to configure that. If not present, defaults to plain "info".
/// Registers two "tracer processor", one for logging to stdout and one that exports to a collector
/// Also registers a panic hook to try to export traces on panics
/// Pay attention to the limits, specially max 1024 events (logs) per span and try to only create spans
/// useful to debugging
/// Careful with span creations in loops as they can make the output hard to read or too big to visualize
pub fn setup_or_panic(
    service_name: String,
    environment: String,
    collector_endpoint: String,
    sample_rate_0_to_1: f64,
) -> TraceShutdownGuard {
    if service_name.trim().is_empty() {
        panic!("Service name can't be empty.");
    }
    if environment.trim().is_empty() {
        panic!("Environment can't be empty. Example: local, dev, stage, prod");
    }
    if sample_rate_0_to_1 > 1. || sample_rate_0_to_1 < 0. {
        panic!("Sample rate should be between 0 and 1");
    }
    let sample_rate_perc = sample_rate_0_to_1 * 100.;
    let service_name_with_env = format!("{service_name}-{environment}");
    println!(
        "Initializing tracing for service: {service_name_with_env}, \
    sampling at: {sample_rate_perc:.0}%, \
    sending to collector at: {}",
        collector_endpoint
    );
    static ONCE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    match ONCE.set(true) {
        Ok(_) => {
            println!("Initializing tracing for {}", &service_name_with_env);
            setup_or_panic_impl(
                service_name_with_env,
                collector_endpoint,
                sample_rate_0_to_1,
            )
        }
        Err(_) => {
            panic!("Tried to initialize tracing again, please, don't do this");
        }
    }
}

pub fn setup_tracing_console_logging_for_test() {
    let filter = {
        let env_filter = EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|e| {
            let default_filter = "info";
            println!(
                "Missing or invalid RUST_LOG, defaulting to {default_filter}. {:#?}",
                e
            );
            EnvFilter::builder()
                .parse(default_filter)
                .unwrap_or_else(|_| panic!("{default_filter} should work as filter"))
        });
        println!("Using env filter: {}", env_filter);
        env_filter
    };
    let fmt = tracing_subscriber::fmt::layer()
        // for tests ansi if nice
        .with_ansi(true)
        .compact()
        .with_filter(filter);
    let subscriber = tracing_subscriber::Registry::default().with(fmt);
    subscriber::set_global_default(subscriber).unwrap();
}

/// Why a custom non-otel subscriber/exporter?
/// 1. Grouping of spans into a trace at service level
/// 2. Alerting or sending "standalone" events
/// 3. Tail sampling, possible due to 1
/// 4. Send service health data
/// 4.1 Health check, heart beat
/// 4.2 Spans open for too long, maybe forgotten
/// 5. Limit on Span+Event count per trace
/// 5.1 When hit stop recording new events or spans for that trace
/// 6. Limit on total Span+Events per minute per TopLevelSpan
/// Change log level for full trace
pub struct TracerTracingSubscriber {
    /// The UUID generated on startup used to track how many instance of this service there are
    uuid: String,
    service_name: String,
    active_trace_storage: Arc<RwLock<storage::ActiveTraceStorage>>,
    exporter: Arc<export::TracerExporter>,
    sampler: Arc<RwLock<sampling::TracerSampler>>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TracesHealthSnapshot {
    time_period_seconds: u64,
    dropped_export_buffer_count: HashMap<String, u32>,
    // Per trace name count of dropped traces due to rate limiting
    dropped_rate_limiting_trace_count: HashMap<String, u32>,
    orphan_events: Vec<SpanEvent>,
}

struct LevelConfig {
    pub raw_rust_log_env_var: String,
    pub global_level: String,
    pub per_trace: HashMap<String, String>,
}

impl TracerTracingSubscriber {
    fn new(
        collector_url: String,
        export_timeout: Duration,
        max_span_plus_event_in_storage: usize,
        span_plus_event_per_minute_per_trace_limit: i64,
    ) -> Self {
        let tracer = Self {
            uuid: "some".to_string(),
            service_name: "some_service_name".to_string(),
            active_trace_storage: Arc::new(RwLock::new(storage::ActiveTraceStorage::new())),
            exporter: Arc::new(export::TracerExporter::new(collector_url, export_timeout)),
            sampler: Arc::new(RwLock::new(sampling::TracerSampler::new(
                max_span_plus_event_in_storage,
                span_plus_event_per_minute_per_trace_limit,
            ))),
        };

        let status_data_reader = StatusDataExporter {
            uuid: tracer.uuid.clone(),
            service_name: tracer.service_name.clone(),
            active_trace_storage: Arc::clone(&tracer.active_trace_storage),
            exporter: Arc::clone(&tracer.exporter),
            sampler: Arc::clone(&tracer.sampler),
        };
        let status_export_task = async move {
            loop {
                let status_data = status_data_reader.get();
                status_data_reader.exporter.export_status(status_data).await;
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        };
        tokio::spawn(status_export_task);
        tracer
    }
    fn span_parent<S: Subscriber + for<'a> LookupSpan<'a>>(
        attrs: &Attributes<'_>,
        ctx: Context<'_, S>,
    ) -> Option<Id> {
        let explicit_root = attrs.is_root();
        let explicit_parent = attrs.parent();
        let is_contextual = attrs.is_contextual();
        let current_active_span = ctx.current_span();
        let current_active_span = current_active_span.id();
        return if let Some(parent) = explicit_parent {
            Some(parent.clone())
        } else if explicit_root {
            None
        } else {
            assert!(
                is_contextual,
                "span wasn't explicit set as root, or had explicit parent and wasn't contextual"
            );
            current_active_span.cloned()
        };
    }

    fn event_span<S: Subscriber + for<'a> LookupSpan<'a>>(
        event: &Event<'_>,
        ctx: Context<'_, S>,
    ) -> Option<Id> {
        let explicit_root = event.is_root();
        let explicit_parent = event.parent();
        let is_contextual = event.is_contextual();
        let current_active_span = ctx.current_span();
        let current_active_span = current_active_span.id();
        return if let Some(parent) = explicit_parent {
            Some(parent.clone())
        } else if explicit_root {
            None
        } else {
            assert!(
                is_contextual,
                "event wasn't explicit set as root, or had explicit parent and wasn't contextual"
            );
            current_active_span.cloned()
        };
    }
}

struct StatusDataExporter {
    uuid: String,
    service_name: String,
    active_trace_storage: Arc<RwLock<storage::ActiveTraceStorage>>,
    exporter: Arc<export::TracerExporter>,
    sampler: Arc<RwLock<sampling::TracerSampler>>,
}

impl StatusDataExporter {
    fn get(&self) -> StatusData {
        let mut w_active_trace_storage = self.active_trace_storage.write();
        let orphan_events = w_active_trace_storage.take_orphan_events();
        let active_traces = w_active_trace_storage.trace_summary();
        let export_queue = self.exporter.get_queue_summary();
        let sampler_status = self.sampler.read().get_sampler_status();
        let errors = self.exporter.take_errors();
        StatusData {
            service_name: self.service_name.clone(),
            sampler_status,
            errors,
            active_traces,
            export_queue,
            orphan_events,
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

use crate::export::Exporter;
use crate::sampling::Sampler;
use crate::storage::SpanOrRootMut;
use storage::RawTracerStorage;

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for TracerTracingSubscriber {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let (span_name, parent_id) = {
            let span_name = attrs.metadata().name().to_string();
            let parent_id = Self::span_parent(attrs, ctx);
            (span_name, parent_id)
        };
        let mut w_active_trace_storage = self.active_trace_storage.write();
        let mut active_summary = w_active_trace_storage.trace_summary();
        let queue_summary = self.exporter.get_queue_summary();
        active_summary.extend_from_slice(&queue_summary);
        let orphan_events_len = w_active_trace_storage.get_orphan_events_len();
        let existing_traces = active_summary;
        let mut w_sampler = self.sampler.write();

        match &parent_id {
            None => {
                // is itself root
                if w_sampler.allow_new_trace(&span_name, &existing_traces, orphan_events_len) {
                    w_active_trace_storage
                        .push_root_span(id.clone(), span_name)
                        .expect("bug: pushing new trace failed");
                }
            }
            Some(parent_id) => {
                // not root, part of existing trace
                let Some(trace) = w_active_trace_storage.get_root_of_mut(parent_id) else{
                  // trace for this span is not in our records
                    return;
                };
                // if trace is partial, we don't record data for it anymore
                if trace.partial {
                    return;
                }
                if w_sampler.allow_new_span(&span_name, &existing_traces, orphan_events_len) {
                    w_active_trace_storage
                        .try_push_child_span(id.clone(), span_name, parent_id.clone())
                        .expect("bug: pushing new child span for non-partial existing trace");
                } else {
                    trace.partial = true;
                }
            }
        };
    }
    fn on_record(&self, _span: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {
        println!("on record");
    }
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let span_event = {
            let mut my_v = MyV { message: None };
            event.record(&mut my_v);
            let name = if let Some(msg) = my_v.message {
                msg
            } else {
                println!("Empty event");
                return;
            };
            SpanEvent {
                name,
                timestamp: u64::try_from(chrono::Utc::now().timestamp_nanos()).expect("to fit u64"),
                level: "".to_string(),
                key_vals: Default::default(),
            }
        };
        let mut w_active_trace_storage = self.active_trace_storage.write();
        let mut w_sampler = self.sampler.write();
        let queue_summary = self.exporter.get_queue_summary();
        let mut active_summary = w_active_trace_storage.trace_summary();
        active_summary.extend_from_slice(&queue_summary);
        let orphan_events_len = w_active_trace_storage.get_orphan_events_len();
        let existing_traces = active_summary;
        let Some(span_id) = Self::event_span(event, ctx) else {
            if w_sampler.allow_new_orphan_event(&existing_traces, orphan_events_len){
                w_active_trace_storage.push_orphan_event(span_event);
            }
            return;
        };
        let Some(trace) =  w_active_trace_storage
            .get_root_of_mut(&span_id) else{
            return;
        };
        if trace.partial {
            return;
        }

        if w_sampler.allow_new_event(&trace.name, &existing_traces, orphan_events_len) {
            w_active_trace_storage
                .try_push_event(&span_id, span_event)
                .expect("bug: pushing new child event for non-partial existing trace");
        } else {
            trace.partial = true;
        }
    }
    fn on_close(&self, span_id: Id, _ctx: Context<'_, S>) {
        let mut w_active_trace_storage = self.active_trace_storage.write();
        let Some(span_or_trace) = w_active_trace_storage.get_mut(&span_id) else {
            return;
        };
        match span_or_trace {
            SpanOrRootMut::Trace(trace) => {
                let now = u64::try_from(chrono::Utc::now().timestamp_nanos())
                    .expect("timestamp to fix u64");
                trace.duration = now.saturating_sub(trace.start);
            }
            SpanOrRootMut::Span(span) => {
                let now = u64::try_from(chrono::Utc::now().timestamp_nanos())
                    .expect("timestamp to fix u64");
                span.duration = now.saturating_sub(span.start);
            }
        }
        let Some(trace) = w_active_trace_storage.remove(&span_id) else{
            // not root
            return;
        };
        let trace = export::Trace {
            service_name: self.service_name.clone(),
            id: trace.id.into_non_zero_u64(),
            name: trace.name,
            partial: trace.partial,
            start: trace.start,
            duration: trace.duration,
            key_vals: trace.key_vals,
            events: trace.events,
            children: trace
                .children
                .into_values()
                .map(|s| export::Span {
                    id: s.id.into_non_zero_u64(),
                    name: s.name,
                    parent_id: s.parent_id.into_non_zero_u64(),
                    start: s.start,
                    duration: s.duration,
                    key_vals: s.key_vals,
                    events: s.events,
                })
                .collect(),
        };
        self.exporter.add_to_queue(trace);
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct CurrentFilters {
    global: String,
    per_crate: HashMap<String, String>,
    per_span: HashMap<String, String>,
}

impl CurrentFilters {
    fn to_filter(&self) -> Result<EnvFilter, ParseError> {
        let as_str = self.to_filter_str();
        EnvFilter::builder().parse(as_str)
    }
    fn to_filter_str(&self) -> String {
        let per_crate = self
            .per_crate
            .iter()
            .map(|(crate_name, filter)| format!("{crate_name}={filter}"))
            .collect::<Vec<String>>()
            .join(",");
        let per_span = self
            .per_span
            .iter()
            .map(|(span_name, filter)| format!("[{span_name}]={filter}"))
            .collect::<Vec<String>>()
            .join(",");

        let non_empty: Vec<String> = vec![self.global.clone(), per_crate, per_span]
            .into_iter()
            .filter(|e| !e.is_empty())
            .collect();
        let filters = non_empty.join(",");
        filters
    }
}

#[derive(Clone)]
struct TracerServerApi {
    reload_handle: Handle<EnvFilter, Registry>,
    current_filters: Arc<RwLock<CurrentFilters>>,
}

#[derive(serde::Deserialize)]
struct NewFilters {
    global: String,
    span_name_to_filter: HashMap<String, String>,
}
#[derive(Debug)]
pub struct ApiError {
    pub code: StatusCode,
    pub message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.code, self.message).into_response()
    }
}

async fn reload_filters(
    tracer: axum::extract::State<TracerServerApi>,
    new_filters: axum::Json<NewFilters>,
) -> Result<axum::Json<CurrentFilters>, ApiError> {
    let mut filters = tracer.current_filters.read().deref().clone();
    filters.global = new_filters.global.to_string();
    for (span_name, filter) in &new_filters.span_name_to_filter {
        filters
            .per_span
            .insert(span_name.to_string(), filter.to_string());
    }
    let updated_filters = filters;
    let filters = updated_filters
        .to_filter()
        .map_err(|e| format!("Failed to create filter: {:#?}", e));
    return match filters {
        Ok(new_filters) => {
            tracer
                .reload_handle
                .reload(new_filters)
                .expect("to be able to reload");
            Ok(axum::Json(updated_filters))
        }
        Err(e) => Err(ApiError {
            code: StatusCode::BAD_REQUEST,
            message: format!(
                "Filter failed to be parsed: {:#?} as str: {}, {e}",
                updated_filters,
                updated_filters.to_filter_str()
            ),
        }),
    };
}

impl TracerServerApi {
    async fn start(self) {
        let app = axum::Router::new()
            .route("/reload_filter", axum::routing::post(reload_filters))
            .with_state(self)
            .layer(tower_http::cors::CorsLayer::very_permissive());
        tokio::spawn(async move {
            axum::Server::bind(
                &format!("0.0.0.0:{}", 4017)
                    .parse()
                    .expect("should be able to api server desired address and port"),
            )
            .serve(app.into_make_service())
            .await
            .unwrap()
        });
    }
}

pub async fn setup_or_panic_impl_2() {
    let filter = EnvFilter::builder().parse("info".to_string()).unwrap();
    let (reloadable_filter, reload_handle) = tracing_subscriber::reload::Layer::new(filter);
    let tracer = TracerTracingSubscriber::new(
        "http://127.0.0.1:4200".to_string(),
        Duration::from_secs(2),
        5000,
        100,
    );
    let trace_with_filter = tracer.with_filter(reloadable_filter);
    let registry = Registry::default().with(trace_with_filter);
    subscriber::set_global_default(registry).unwrap();
    let tracer_api = TracerServerApi {
        reload_handle,
        current_filters: Arc::new(RwLock::new(CurrentFilters {
            global: "info".to_string(),
            per_crate: Default::default(),
            per_span: Default::default(),
        })),
    };
    tracer_api.start().await;
}

#[tokio::test]
async fn a() {
    setup_or_panic_impl_2().await;
    loop {
        {
            info!("SomeEvent");
            error_span!("SomeSpan").in_scope(|| {
                trace!("Trace");
                debug!("Debug");
                info!("Info");
                warn!("Warn");
                error!("Error");
                info_span!("SomeOtherSpan2");
            });
        }
        tokio::time::sleep(Duration::from_secs(15)).await;
    }
    // tracer.0.traces.print();
    // tracer.0.traces_health.print();
    // let guard = tracer.0.root_span_to_spans.read();
    // let root_span_to_spans = guard.deref().clone();
    // let guard = tracer.0.span_to_root_span.read();
    // let span_to_root_span = guard.deref().clone();
    // println!("{:#?}", root_span_to_spans.len());
    // println!("{:#?}", span_to_root_span.len());
}

fn setup_or_panic_impl(
    service_name_with_env: String,
    collector_endpoint: String,
    sample_rate_0_to_1: f64,
) -> TraceShutdownGuard {
    if service_name_with_env.trim().is_empty() {
        panic!("Service name shouldn't be empty!");
    }
    let open_tel_tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_batch_config(
            // we want to allow big traces for nightly background jobs for example
            // up to 500MB max for a single trace but also have a long queue
            // to not drop the smaller ones.
            // Most of these are the same as the default, but keeping it here
            // so it doesnt change on lib updates since this is important
            BatchConfig::default()
                .with_max_queue_size(2048)
                .with_scheduled_delay(Duration::from_secs(5))
                .with_max_export_batch_size(512)
                .with_max_export_timeout(Duration::from_secs(30))
                .with_max_concurrent_exports(4),
        )
        .with_trace_config(
            trace::Config::default()
                // we don't want to lose any event, if possible
                .with_max_events_per_span(500_000)
                .with_resource(opentelemetry::sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", service_name_with_env),
                ]))
                .with_sampler(trace::Sampler::TraceIdRatioBased(sample_rate_0_to_1)),
        )
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(collector_endpoint),
        )
        .install_batch(opentelemetry::runtime::Tokio)
        .unwrap();

    // closure because filter is not clone
    let get_filter = || {
        let env_filter = EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|e| {
            let default_filter = "info";
            println!(
                "Missing or invalid RUST_LOG, defaulting to {default_filter}. {:#?}",
                e
            );
            EnvFilter::builder()
                .parse(default_filter)
                .unwrap_or_else(|_| panic!("{default_filter} should work as filter"))
        });
        // make sure we can log panics
        let final_filter = env_filter.add_directive(
            Directive::from_str(&format!("{}=info", env!("CARGO_CRATE_NAME")))
                .expect("to be a valid filter"),
        );
        println!("Using env filter: {}", final_filter);
        final_filter
    };
    let fmt = tracing_subscriber::fmt::layer()
        // we normally look up logs in dash or kibana and it doesnt handle ansi, dash
        // throws it away, kibana shows weird characters
        // see: https://github.com/kubernetes/dashboard/issues/1035
        .with_ansi(false)
        .json()
        .with_filter(get_filter());
    let open_tel = tracing_opentelemetry::layer()
        // remove these extra attributes because they are generated
        // for _each_ span and event, generating _a lot_ of attributes
        // per event and span
        .with_threads(false)
        .with_tracked_inactivity(false)
        .with_location(false)
        .with_exception_fields(false)
        .with_exception_field_propagation(false)
        .with_tracer(open_tel_tracer.clone())
        .with_filter(get_filter());

    install_global_export_traces_on_panic_hook();
    let subscriber = tracing_subscriber::Registry::default()
        .with(open_tel)
        .with(fmt);
    subscriber::set_global_default(subscriber).unwrap();
    TraceShutdownGuard {
        tracer: open_tel_tracer,
    }
}

fn install_global_export_traces_on_panic_hook() {
    let current = std::panic::take_hook();
    println!("Installing panic hook");
    std::panic::set_hook(Box::new(move |panic_info| {
        println!("Running panic hook, trying to export creating and exporting panic span.");
        // Make sure we signal that we panic
        let panic_span = tracing::info_span!("program panicked", is_panic = true);
        panic_span.in_scope(|| {
            let bt = std::backtrace::Backtrace::force_capture();
            let panic_info: String = panic_info.to_string().chars().take(28_000).collect();
            let bt: String = bt.to_string().chars().take(28_000).collect();
            tracing::error!("Code panicked: Panic info: {}.", panic_info);
            tracing::error!("Backtrace:\n{bt}.");
        });
        current(panic_info)
    }));
}
