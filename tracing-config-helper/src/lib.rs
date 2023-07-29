//! This serves as an unified config for projects  
//! It outputs pretty logs to the console stdout and stderr,
//! but also exports traces to a collector
//!

use api_structs::exporter::{
    ClosedSpan, Config, Level, NewOrphanEvent, NewSpan, NewSpanEvent, SamplerLimits, SpanEvent,
    TracerFilters,
};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::DerefMut;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tracing::field::{Field, Visit};
use tracing::span::Record;
use tracing::subscriber::{self};
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::filter::ParseError;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::{LookupSpan, SpanRef};
use tracing_subscriber::reload::Handle;
use tracing_subscriber::{EnvFilter, Layer, Registry};

mod old;
mod sampling;
pub use old::*;

pub fn print_if_dbg<T: AsRef<str>>(debug_statement: T) {
    if debugging() {
        println!("{}", debug_statement.as_ref());
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
    sampler: Arc<parking_lot::RwLock<sampling::TracerSampler>>,
    subscriber_event_sender: Sender<SubscriberEvent>,
}

impl TracerTracingSubscriber {
    fn new(
        sampler_limits: SamplerLimits,
        // span_plus_event_per_minute_per_trace_limit: u32,
        // orphan_events_per_minute_limit: u32,
        subscriber_event_sender: Sender<SubscriberEvent>,
    ) -> Self {
        let sampler = Arc::new(parking_lot::RwLock::new(sampling::TracerSampler::new(
            sampler_limits,
        )));
        let tracer = Self {
            // active_trace_storage,
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
        match self.subscriber_event_sender.try_send(subscriber_event) {
            Ok(_) => {}
            Err(_e) => {
                println!("Send failed!");
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

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for TracerTracingSubscriber {
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("entered span to exist!");
        let mut extensions = span.extensions_mut();
        let tracer_span_data: Option<&mut TracerSpanData> = extensions.get_mut();
        if tracer_span_data.is_some() {
            // span already entered
            return;
        } else {
            extensions.insert(TracerSpanData {
                first_entered_at: std::time::Instant::now(),
            });
        }
        drop(extensions);
        let name = span.name();
        println!("{} entered", name);
        let root_span = Self::span_root(id.clone(), &ctx).expect("root span to exist");

        if root_span.id() == *id {
            println!("Was root");
            // we are root of a new trace
            let mut w_sampler = self.sampler.write();
            let new_trace_allowed = w_sampler.allow_new_trace(&root_span.name());
            drop(w_sampler);
            if new_trace_allowed {
                self.send_subscriber_event_to_export(SubscriberEvent::NewSpan(NewSpan {
                    id: id.into_non_zero_u64(),
                    trace_id: root_span.id().into_non_zero_u64(),
                    name: name.to_string(),
                    parent_id: None,
                    start: u64::try_from(chrono::Utc::now().naive_utc().timestamp_nanos()).unwrap(),
                    key_vals: Default::default(),
                }));
                root_span
                    .extensions_mut()
                    .insert(TracerRootSpanData { dropped: false })
            } else {
                root_span
                    .extensions_mut()
                    .insert(TracerRootSpanData { dropped: true })
            }
        } else {
            // we are not root, check if current trace was dropped
            println!("Was not root");
            if Self::trace_was_dropped(id.clone(), &ctx) {
                println!("Was dropped");
                return;
            } else {
                let mut w_sampler = self.sampler.write();
                let new_span_allowed = w_sampler.allow_new_span(root_span.name());
                drop(w_sampler);
                if new_span_allowed {
                    println!("Was allowed");
                    self.send_subscriber_event_to_export(SubscriberEvent::NewSpan(NewSpan {
                        id: id.into_non_zero_u64(),
                        trace_id: root_span.id().into_non_zero_u64(),
                        name: name.to_string(),
                        parent_id: Some(root_span.id().into_non_zero_u64()),
                        start: u64::try_from(chrono::Utc::now().naive_utc().timestamp_nanos())
                            .unwrap(),
                        key_vals: Default::default(),
                    }));
                } else {
                    println!("Was not allowed");
                }
            }
        }
    }
    fn on_record(&self, _span: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {
        println!("on record");
    }
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let span = ctx.event_span(event);

        let event_data = {
            let mut my_v = MyV { message: None };
            event.record(&mut my_v);
            let name = if let Some(msg) = my_v.message {
                msg
            } else {
                println!("ALERT: Empty events are not supported!");
                return;
            };
            let level = match event.metadata().level() {
                &tracing::metadata::Level::TRACE => Level::Trace,
                &tracing::metadata::Level::DEBUG => Level::Debug,
                &tracing::metadata::Level::INFO => Level::Info,
                &tracing::metadata::Level::WARN => Level::Warn,
                &tracing::metadata::Level::ERROR => Level::Error,
            };
            SpanEvent {
                name,
                timestamp: u64::try_from(chrono::Utc::now().timestamp_nanos()).expect("to fit u64"),
                level,
                key_vals: Default::default(),
            }
        };

        let Some(span) = span else {
            let mut w_sampler = self.sampler.write();
            let new_orphan_event_allowed = w_sampler.allow_new_orphan_event();
            drop(w_sampler);
            if new_orphan_event_allowed{
                self.send_subscriber_event_to_export(SubscriberEvent::NewOrphanEvent(NewOrphanEvent{
                    name: event_data.name,
                    timestamp: event_data.timestamp,
                    level: event_data.level,
                    key_vals: event_data.key_vals,
                }));
            }
            return;
        };
        if Self::trace_was_dropped(span.id(), &ctx) {
            return;
        }
        let mut w_sampler = self.sampler.write();
        let new_event_allowed = w_sampler.allow_new_event(
            Self::span_root(span.id(), &ctx)
                .expect("root span to exist")
                .name(),
        );
        drop(w_sampler);
        if new_event_allowed {
            self.send_subscriber_event_to_export(SubscriberEvent::NewSpanEvent(NewSpanEvent {
                span_id: span.id().into_non_zero_u64(),
                name: event_data.name,
                timestamp: event_data.timestamp,
                level: event_data.level,
                key_vals: event_data.key_vals,
            }));
        }
    }
    fn on_close(&self, span_id: Id, ctx: Context<'_, S>) {
        let span = ctx.span(&span_id).expect("span to exist if it got closed");
        if Self::trace_was_dropped(span.id(), &ctx) {
            return;
        }
        let extensions = span.extensions();
        let tracer_span_data: &TracerSpanData = extensions
            .get()
            .expect("tracer span data to exist if span is closing");
        self.send_subscriber_event_to_export(SubscriberEvent::ClosedSpan(ClosedSpan {
            id: span_id.into_non_zero_u64(),
            duration: u64::try_from(tracer_span_data.first_entered_at.elapsed().as_nanos())
                .expect("duration to fit u64"),
        }));
    }
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

pub fn to_filter(filters: &TracerFilters) -> Result<EnvFilter, ParseError> {
    let as_str = filters.to_filter_str();
    EnvFilter::builder().parse(as_str)
}

use tokio::sync::mpsc::{Receiver, Sender};
use tokio_tungstenite;

#[derive(Debug, Clone)]
pub enum SubscriberEvent {
    NewSpan(NewSpan),
    NewSpanEvent(NewSpanEvent),
    ClosedSpan(ClosedSpan),
    NewOrphanEvent(NewOrphanEvent),
}

struct CollectorConnection {
    config: Config,
    reload_handle: Handle<EnvFilter, Registry>,
    subscriber_receiver: Receiver<SubscriberEvent>,
    url: String,
    sampler: Arc<parking_lot::RwLock<TracerSampler>>,
}

use api_structs::exporter::ApplicationToCollectorMessage;
use api_structs::exporter::CollectorToApplicationMessage;

use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use api_structs::websocket::client_adapter::ReqResp;
use api_structs::websocket::{Error2, Message, ReqRespSender};

pub async fn handle(
    msg: CollectorToApplicationMessage,
    config: Arc<Config>,
    sampler: Arc<parking_lot::RwLock<TracerSampler>>,
    filter_reload_handle: Arc<Handle<EnvFilter, Registry>>,
) -> ApplicationToCollectorMessage {
    match msg {
        CollectorToApplicationMessage::GetConfig => {
            println!("Got Config Request");
            ApplicationToCollectorMessage::GetConfigResponse((*config).clone())
        }
        CollectorToApplicationMessage::GetStats => {
            ApplicationToCollectorMessage::GetStatsResponse(sampler.write().get_tracer_stats())
        }
        CollectorToApplicationMessage::ChangeFilters(new_filters) => {
            match new_filters.to_env_filter() {
                Ok(new_env_filter) => {
                    let new_filter = new_env_filter.to_string();
                    filter_reload_handle
                        .reload(new_env_filter)
                        .expect("reload to work");
                    ApplicationToCollectorMessage::ChangeFiltersResponse(Ok(new_filter))
                }
                Err(invalid_filter) => {
                    ApplicationToCollectorMessage::ChangeFiltersResponse(Err(invalid_filter))
                }
            }
        }
        CollectorToApplicationMessage::Pong => {
            panic!("Got unexpected Pong message");
        }
        CollectorToApplicationMessage::Ack => {
            panic!("Got unexpected Ack message");
        }
    }
}

pub struct TelemetrySender;

impl TelemetrySender {
    async fn try_send_until_until_success(event: SubscriberEvent, ws_sender: &mut ReqResp) {
        loop {
            let send_result = match &event {
                SubscriberEvent::NewSpan(new_span) => {
                    print_if_dbg("Got new span to export");
                    ws_sender
                        .send(Message::new(ApplicationToCollectorMessage::NewSpan(
                            new_span.clone(),
                        )))
                        .await
                }
                SubscriberEvent::NewSpanEvent(event) => {
                    print_if_dbg("Got new span_event to export");
                    ws_sender
                        .send(Message::new(ApplicationToCollectorMessage::NewSpanEvent(
                            event.clone(),
                        )))
                        .await
                }
                SubscriberEvent::ClosedSpan(closed_span) => {
                    print_if_dbg("Got new closed_span to export");
                    ws_sender
                        .send(Message::new(ApplicationToCollectorMessage::ClosedSpan(
                            closed_span.clone(),
                        )))
                        .await
                }
                SubscriberEvent::NewOrphanEvent(orphan_event) => {
                    print_if_dbg("Got new orphan_event to export");
                    ws_sender
                        .send(Message::new(ApplicationToCollectorMessage::NewOrphanEvent(
                            orphan_event.clone(),
                        )))
                        .await
                }
            };
            let receive_handle = match send_result {
                Ok(receive_handle) => receive_handle,
                Err(send_err) => {
                    println!("Error sending telemetry data: {:?}", send_err);
                    continue;
                }
            };
            match receive_handle.await {
                Ok(msg) => {
                    if msg.data.is_ack() {
                        print_if_dbg("Got ACK back");
                        return;
                    } else {
                        println!(
                            "Expected ack got: {:#?} when sending telemetry data",
                            msg.data
                        );
                    }
                }
                Err(e) => {
                    println!(
                        "Error receiving response after sending telemetry data, got {:?}",
                        e
                    );
                }
            }
            print_if_dbg("Waiting 1s before trying again");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    pub async fn send_loop(
        telemetry_event_receiver: Arc<tokio::sync::RwLock<ReceiverStream<SubscriberEvent>>>,
        mut ws_sender: ReqResp,
        flush_stream: Arc<RwLock<ReceiverStream<FlushRequest>>>,
    ) {
        enum Event {
            Subscriber(SubscriberEvent),
            Flush(FlushRequest),
        }
        let mut w_telemetry_event_receiver = telemetry_event_receiver
            .try_write()
            .expect("no two subscriber_receiver to exist a given time");
        let mut w_flush_stream = flush_stream
            .try_write()
            .expect("no two flush_stream to exist a given time");
        let mut stream = w_telemetry_event_receiver
            .deref_mut()
            .map(|a| Event::Subscriber(a))
            .merge(
                w_flush_stream
                    .deref_mut()
                    .map(|flush_request| Event::Flush(flush_request)),
            );

        while let Some(event) = stream.next().await {
            match event {
                Event::Subscriber(subscriber_msg) => {
                    // send it normally
                    Self::try_send_until_until_success(subscriber_msg, &mut ws_sender).await;
                }
                Event::Flush(flush_request) => {
                    print_if_dbg("Got flush request");
                    // process all items immediately available, waiting at most 100ms for new items
                    loop {
                        match tokio::time::timeout(Duration::from_millis(100), stream.next()).await
                        {
                            Ok(Some(event)) => match event {
                                Event::Subscriber(subscriber_msg) => {
                                    print_if_dbg("Sending queued event as part of flushing");
                                    Self::try_send_until_until_success(
                                        subscriber_msg,
                                        &mut ws_sender,
                                    )
                                    .await;
                                }
                                Event::Flush(_flush_request) => print_if_dbg(
                                    "Logic error, tried to flush when was already flushing",
                                ),
                            },
                            Ok(None) | Err(_) => {
                                print_if_dbg("Done flushing, sending done");
                                // channels closed
                                if let Err(_e) = flush_request.responder.send(()) {
                                    print_if_dbg("Flush request receiver gone after flush");
                                }
                                print_if_dbg("All done flushing and notifying it");
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}
impl CollectorConnection {
    pub async fn try_connect_in_loop(
        url: String,
        ws_timeout: Duration,
    ) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
        let mut config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
        config.write_buffer_size = 128 * 1024;
        config.max_write_buffer_size = 1024 * 1024; // 1MB

        let (ws, _) = {
            loop {
                let connection_fut =
                    tokio_tungstenite::connect_async_with_config(&url, Some(config), false);
                let connection_res = match tokio::time::timeout(ws_timeout, connection_fut).await {
                    Ok(connection_res) => connection_res,
                    Err(_e) => {
                        println!("Timeout connecting to Collector");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                match connection_res {
                    Ok(connected) => {
                        println!("Connected!");
                        break connected;
                    }
                    Err(e) => {
                        println!("Error connecting to Collector: {}", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                };
            }
        };
        ws
    }
    pub async fn ping_once(sender: &mut ReqResp) -> Result<(), Error2> {
        let ping_msg_response = match sender
            .send(Message::new(ApplicationToCollectorMessage::Ping))
            .await
        {
            Ok(ping_msg_response) => ping_msg_response,
            Err(e) => {
                return Err(e);
            }
        };
        let msg = match ping_msg_response.await {
            Ok(msg) => msg,
            Err(e) => {
                return Err(e);
            }
        };
        return if let CollectorToApplicationMessage::Pong = msg.data {
            Ok(())
        } else {
            Err(Error2::Other(format!(
                "Got unexpected message for ping: {:?}",
                msg.data
            )))
        };
    }
    pub async fn ping_loop(period: Duration, req_response: ReqResp) -> Error2 {
        let mut sender = req_response.clone();
        loop {
            tokio::time::sleep(period).await;
            if let Err(e) = Self::ping_once(&mut sender).await {
                return e;
            }
        }
    }

    pub async fn run_main_loop(
        self,
        ws_timeout: Duration,
        ping_interval: Duration,
        flush_request_stream: ReceiverStream<FlushRequest>,
    ) {
        let config = Arc::new(self.config);
        let sampler = self.sampler;
        let telemetry_events = Arc::new(tokio::sync::RwLock::new(
            tokio_stream::wrappers::ReceiverStream::new(self.subscriber_receiver),
        ));
        let flush_request_stream = Arc::new(tokio::sync::RwLock::new(flush_request_stream));
        let filter_reload_handle = Arc::new(self.reload_handle);
        let timeout = ws_timeout;
        pub struct CollectorTask {
            ws_server_task: tokio::task::JoinHandle<()>,
            pinger_task: tokio::task::JoinHandle<Error2>,
            telemetry_sender_loop: tokio::task::JoinHandle<()>,
        }
        loop {
            let socket = Self::try_connect_in_loop(self.url.clone(), ws_timeout).await;
            let (ws_server_task, req_resp) =
                ReqResp::new_tracer_client(timeout, timeout, socket, {
                    let config = Arc::clone(&config);
                    let sampler = Arc::clone(&sampler);
                    let filter_reload_handle = Arc::clone(&filter_reload_handle);
                    Box::new(move |msg| {
                        Box::pin(handle(
                            msg,
                            Arc::clone(&config),
                            Arc::clone(&sampler),
                            Arc::clone(&filter_reload_handle),
                        ))
                    })
                });
            let pinger = tokio::spawn(Self::ping_loop(ping_interval, req_resp.clone()));
            let telemetry_sender_loop = tokio::spawn(TelemetrySender::send_loop(
                Arc::clone(&telemetry_events),
                req_resp.clone(),
                Arc::clone(&flush_request_stream),
            ));
            // if pinger returned it means we failed the ping check, reconnect
            match pinger.await {
                Ok(e) => {
                    println!("Ping failed: {:?}", e);
                }
                Err(e) => {
                    println!("Ping failed: {:?}", e);
                }
            }
            sampler.write().register_reconnect();
            // ping failed
            telemetry_sender_loop.abort();
            ws_server_task.abort();
            // wait at least 1 sec before reconnecting
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    pub fn new(
        service_name: String,
        env: String,
        url: String,
        sampler_limits: SamplerLimits,
        filters: TracerFilters,
        reload_handle: Handle<EnvFilter, Registry>,
        subscriber_receiver: Receiver<SubscriberEvent>,
        tracer_sampler: Arc<parking_lot::RwLock<TracerSampler>>,
    ) -> Self {
        let uuid = uuid::Uuid::new_v4().to_string();
        Self {
            config: Config {
                uuid,
                service_name,
                env,
                filters,
                sampler_limits,
            },
            url,
            reload_handle,
            subscriber_receiver,
            sampler: tracer_sampler,
        }
    }
}

#[test]
fn b() {
    let mut tracer_filters = TracerFilters {
        global: Level::Info,
        per_crate: HashMap::new(),
        per_span: HashMap::new(),
    };
    tracer_filters
        .per_span
        .insert("SomeSpan".to_string(), Level::Debug);
    tracer_filters
        .per_span
        .insert("A".to_string(), Level::Trace);
    tracer_filters.per_span.insert("B".to_string(), Level::Warn);
    let filter = EnvFilter::builder()
        .parse(tracer_filters.to_filter_str())
        .unwrap();
    println!("{}", tracer_filters.to_filter_str());
    println!("{}", filter.to_string());
    return;
}

pub trait ToEnvFilter {
    fn to_env_filter(&self) -> Result<EnvFilter, String>;
}
impl ToEnvFilter for TracerFilters {
    fn to_env_filter(&self) -> Result<EnvFilter, String> {
        let desired_filter_as_str = self.to_filter_str();
        let filter = EnvFilter::builder().parse(&desired_filter_as_str).unwrap();
        let obtained_filter_as_str = filter.to_string();
        if desired_filter_as_str == obtained_filter_as_str {
            Ok(filter)
        } else {
            Err(format!("Desired and obtained filters don't match: {desired_filter_as_str} vs {obtained_filter_as_str}"))
        }
    }
}

pub struct TracerConfig {
    pub collector_url: String,
    pub env: String,
    pub service_name: String,
    pub filters: TracerFilters,
    pub ws_timeout: Duration,
    pub ping_interval: Duration,
    pub sampler_limits: SamplerLimits,
    pub maximum_spe_buffer: u32,
}

pub struct FlushRequest {
    responder: tokio::sync::oneshot::Sender<()>,
}

#[derive(Debug)]
pub struct ExportFlusherHandle {
    sender: tokio::sync::mpsc::Sender<FlushRequest>,
}

#[derive(Debug, Clone)]
pub enum FlushErrors {
    FlushReceiverGone,
    Timeout,
}

impl ExportFlusherHandle {
    pub fn new() -> (ExportFlusherHandle, ReceiverStream<FlushRequest>) {
        let (sender, receiver) = tokio::sync::mpsc::channel::<FlushRequest>(1);
        let request_stream = tokio_stream::wrappers::ReceiverStream::new(receiver);
        (Self { sender }, request_stream)
    }
    async fn send_new_request(&self, timeout: Duration) -> Result<(), FlushErrors> {
        let (flushed_sender, flushed_receiver) = tokio::sync::oneshot::channel::<()>();
        if let Err(e) = self
            .sender
            .send(FlushRequest {
                responder: flushed_sender,
            })
            .await
        {
            print_if_dbg(format!("Error: {}", e));
            return Err(FlushErrors::FlushReceiverGone);
        }
        let Ok(response) = tokio::time::timeout(timeout,flushed_receiver).await else{
            return Err(FlushErrors::Timeout);
        };
        response.map_err(|recv_error| {
            print_if_dbg(format!("Error: {}", recv_error));
            FlushErrors::FlushReceiverGone
        })
    }
    pub async fn flush(&self, timeout: Duration) -> Result<(), FlushErrors> {
        print_if_dbg("Flushing");
        self.send_new_request(timeout).await
    }
}

struct TracerTask {
    flush_handle: ExportFlusherHandle,
    thread_handle: std::thread::JoinHandle<()>,
}
pub async fn setup_tracer_client_or_panic(config: TracerConfig) -> ExportFlusherHandle {
    // we start a new thread and runtime so it can still get data and debug issues involving the main program async
    // runtime starved from CPU time
    let (s, r) = tokio::sync::oneshot::channel::<ExportFlusherHandle>();
    let thread_handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("tracer_thread")
            .build()
            .expect("runtime to be able to start");
        runtime.block_on(async {
            let export_flusher_handle = setup_tracer_client_or_panic_impl(config).await;
            s.send(export_flusher_handle)
                .expect("receiver to exist and parent thread to not have panicked");
            tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
        });
    });
    let tracer_task = TracerTask {
        flush_handle: r.await.unwrap(),
        thread_handle,
    };
    tracer_task.flush_handle
}

async fn setup_tracer_client_or_panic_impl(config: TracerConfig) -> ExportFlusherHandle {
    let spe_buffer_len = usize::try_from(config.maximum_spe_buffer).expect("u32 to fit usize");
    let filter = config
        .filters
        .to_env_filter()
        .expect("provided filter config to be valid");

    let (reloadable_filter, reload_handle) = tracing_subscriber::reload::Layer::new(filter);
    let (subscriber_event_sender, subscriber_event_receiver) =
        tokio::sync::mpsc::channel::<SubscriberEvent>(spe_buffer_len);
    let tracer =
        TracerTracingSubscriber::new(config.sampler_limits.clone(), subscriber_event_sender);
    let tracer_sampler = Arc::clone(&tracer.sampler);
    let collector = CollectorConnection::new(
        config.service_name,
        config.env,
        config.collector_url,
        config.sampler_limits,
        config.filters,
        reload_handle,
        subscriber_event_receiver,
        tracer_sampler,
    );
    let (flush_handle, flush_request_stream) = ExportFlusherHandle::new();
    let _application_to_collector_network_task = tokio::spawn(collector.run_main_loop(
        config.ws_timeout,
        config.ping_interval,
        flush_request_stream,
    ));
    let trace_with_filter = tracer.with_filter(reloadable_filter);
    let registry = Registry::default().with(trace_with_filter);
    subscriber::set_global_default(registry).unwrap();
    flush_handle
}
mod test;
