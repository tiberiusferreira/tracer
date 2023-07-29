use crate::TracerConfig;
use api_structs::exporter::CollectorToApplicationMessage::Pong;
use api_structs::exporter::{
    ApplicationToCollectorMessage, ClosedSpan, CollectorToApplicationMessage, Level,
    NewOrphanEvent, NewSpan, NewSpanEvent, SamplerLimits, TracerFilters,
};
use api_structs::websocket::axum_adapter::ReqResp;
use api_structs::websocket::{Message, ReqRespSender};
use axum::extract::ws::WebSocket;
use axum::extract::{State, WebSocketUpgrade};
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::{debug, error, error_span, info, info_span, trace, warn, warn_span};

struct TestTracerCollector {
    port: u16,
    listen_path: String,
}

#[axum::debug_handler]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(channel): State<Sender<WebSocket>>,
) -> impl axum::response::IntoResponse {
    // finalize the upgrade process by returning upgrade callback.
    // we can customize the callback by sending additional info such as address.
    ws.on_upgrade(move |socket| tracer_client_loop(socket, channel))
}

async fn tracer_client_loop(socket: WebSocket, channel: Sender<WebSocket>) {
    channel.send(socket).await.unwrap();
}

impl TestTracerCollector {
    fn new(api_port: u32, path: &str) -> (JoinHandle<()>, Receiver<WebSocket>) {
        let (sender_application_to_collector, receiver_sender_application_to_collector) =
            tokio::sync::mpsc::channel::<WebSocket>(1);
        let app = axum::Router::new()
            .route(path, axum::routing::get(ws_handler))
            .with_state(sender_application_to_collector)
            .layer(tower_http::cors::CorsLayer::very_permissive());
        let server_handle = tokio::spawn(async move {
            axum::Server::bind(
                &format!("0.0.0.0:{}", api_port)
                    .parse()
                    .expect("should be able to api server desired address and port"),
            )
            .serve(app.into_make_service())
            .await
            .unwrap()
        });
        (server_handle, receiver_sender_application_to_collector)
    }
}

struct MessageExpecter<'a> {
    receiver: &'a mut tokio::sync::mpsc::Receiver<ApplicationToCollectorMessage>,
    max_wait: Duration,
}
impl<'a> MessageExpecter<'a> {
    pub async fn expect_orphan_event(&mut self) -> NewOrphanEvent {
        match tokio::time::timeout(self.max_wait, self.receiver.recv()).await {
            Ok(msg) => match msg.expect("to get msg") {
                ApplicationToCollectorMessage::NewOrphanEvent(new_orphan_event) => new_orphan_event,
                x => {
                    panic!("Got unexpected msg: {:#?}", x);
                }
            },
            Err(_e) => {
                panic!("Timeout");
            }
        }
    }
    pub async fn expect_new_span(&mut self) -> NewSpan {
        match tokio::time::timeout(self.max_wait, self.receiver.recv()).await {
            Ok(msg) => match msg.expect("to get msg") {
                ApplicationToCollectorMessage::NewSpan(new_span) => new_span,
                x => {
                    panic!("Got unexpected msg: {:#?}", x);
                }
            },
            Err(_e) => {
                panic!("Timeout");
            }
        }
    }
    pub async fn expect_closed_span(&mut self) -> ClosedSpan {
        match tokio::time::timeout(self.max_wait, self.receiver.recv()).await {
            Ok(msg) => match msg.expect("to get msg") {
                ApplicationToCollectorMessage::ClosedSpan(closed_span) => closed_span,
                x => {
                    panic!("Got unexpected msg: {:#?}", x);
                }
            },
            Err(_e) => {
                panic!("Timeout");
            }
        }
    }
    pub async fn expect_new_span_event(&mut self) -> NewSpanEvent {
        match tokio::time::timeout(self.max_wait, self.receiver.recv()).await {
            Ok(msg) => match msg.expect("to get msg") {
                ApplicationToCollectorMessage::NewSpanEvent(new_span_event) => new_span_event,
                x => {
                    panic!("Got unexpected msg: {:#?}", x);
                }
            },
            Err(_e) => {
                panic!("Timeout");
            }
        }
    }
}

pub async fn handle(
    msg: ApplicationToCollectorMessage,
    sender: tokio::sync::mpsc::Sender<ApplicationToCollectorMessage>,
    processing_delay: Duration,
) -> CollectorToApplicationMessage {
    tokio::time::sleep(processing_delay).await;
    return match &msg {
        ApplicationToCollectorMessage::Ping => Pong,
        ApplicationToCollectorMessage::NewSpan(_new_span) => {
            sender.send(msg.clone()).await.unwrap();
            CollectorToApplicationMessage::Ack
        }
        ApplicationToCollectorMessage::NewSpanEvent(_new_span_event) => {
            sender.send(msg.clone()).await.unwrap();
            CollectorToApplicationMessage::Ack
        }
        ApplicationToCollectorMessage::ClosedSpan(_closed_span) => {
            sender.send(msg.clone()).await.unwrap();
            CollectorToApplicationMessage::Ack
        }
        ApplicationToCollectorMessage::NewOrphanEvent(_orphan_event) => {
            sender.send(msg.clone()).await.unwrap();
            CollectorToApplicationMessage::Ack
        }
        x => {
            println!("Unexpected msg: {:#?}", x);
            unimplemented!()
        }
    };
}

#[tokio::test]
async fn doesnt_panic_if_server_offline_and_reconnects_eventually() {
    let shutdown_handle = crate::setup_tracer_client_or_panic(TracerConfig {
        collector_url: "ws://127.0.0.1:4200/websocket/collector".to_string(),
        env: "test".to_string(),
        service_name: "test_service".to_string(),
        filters: TracerFilters {
            global: Level::Info,
            per_crate: Default::default(),
            per_span: Default::default(),
        },
        ws_timeout: Duration::from_secs(1),
        ping_interval: Duration::from_secs(1),
        maximum_spe_buffer: 3,
        sampler_limits: SamplerLimits {
            span_plus_event_per_minute_per_trace_limit: 50,
            orphan_events_per_minute_limit: 50,
        },
    })
    .await;
    // tokio::time::sleep(Duration::from_secs(2)).await;
    // for i in 0..10_000 {
    //     info!("Orphan Event");
    //     dummy_spe(i);
    // }
    for i in 0..3 {
        // let (http_server_handle, mut receiver) =
        //     TestTracerCollector::new(4200, "/websocket/collector");
        // println!("New server up");
        //
        // let websocket = match receiver.recv().await {
        //     None => {
        //         panic!("No client connected and server is gone");
        //     }
        //     Some(client) => client,
        // };
        // let timeout = Duration::from_secs(1);
        // let received_messages: Arc<RwLock<VecDeque<ApplicationToCollectorMessage>>> =
        //     Arc::new(RwLock::new(VecDeque::new()));
        //
        // let (websocket_server_handle, mut req_resp) =
        //     ReqResp::new_tracer_server(timeout, timeout, websocket, {
        //         let received_messages = Arc::clone(&received_messages);
        //         Box::new(move |msg| Box::pin(handle(msg, Arc::clone(&received_messages))))
        //     });
        // assert_application_responds_to_config_request(&mut req_resp).await;
        // assert_application_responds_to_stats_request(&mut req_resp).await;
        // println!("Aborting tasks");
        // http_server_handle.abort();
        // websocket_server_handle.abort();
        // println!("Aborted, sleeping for 10s");
        // tokio::time::sleep(Duration::from_secs(10)).await;
        // println!("Done");
    }

    // shutdown_handle.flush().await.ok();
}

#[tokio::test]
async fn doesnt_oom_if_server_hangs() {
    let (_server_handle, mut receiver) = TestTracerCollector::new(4200, "/websocket/collector");

    let shutdown_handle = crate::setup_tracer_client_or_panic(TracerConfig {
        collector_url: "ws://127.0.0.1:4200/websocket/collector".to_string(),
        env: "test".to_string(),
        service_name: "test_service".to_string(),
        filters: TracerFilters {
            global: Level::Info,
            per_crate: Default::default(),
            per_span: Default::default(),
        },
        ws_timeout: Duration::from_secs(1),
        ping_interval: Duration::from_secs(1),
        maximum_spe_buffer: 3,
        sampler_limits: SamplerLimits {
            span_plus_event_per_minute_per_trace_limit: 50,
            orphan_events_per_minute_limit: 50,
        },
    })
    .await;
    let mut collector = match receiver.recv().await {
        None => {
            panic!("No client connected and server is gone");
        }
        Some(client) => client,
    };

    for i in 0..50_000 {
        info!("Dummy event {}", i);
        if i % 10_000 == 0 {
            println!("{i}");
            let usage = memory_stats::memory_stats().unwrap().physical_mem;
            let usage_as_mb = usage / 1_000_000;
            println!("Using {usage_as_mb}MB");
        }
        dummy_spe(i);
    }
    let usage = memory_stats::memory_stats().unwrap().physical_mem;
    let usage_as_mb = usage / 1_000_000;
    println!("Using {usage_as_mb}MB");
    if usage_as_mb > 30 {
        panic!("Using {usage_as_mb}MB");
    }
    // shutdown_handle.flush().await.ok();
}

fn dummy_spe(i: i32) {
    error_span!("Error Span").in_scope(|| {
        trace!("Top level trace event {i}");
        debug!("Top level debug event {i}");
        info!("Top level info event {i}");
        warn!("Top level warn event {i}");
        error!("Top level error event {i}");
        info_span!("Nested Info Span").in_scope(|| {
            trace!("Nested level trace event {i}");
            debug!("Nested level debug event {i}");
            info!("Nested level info event {i}");
            warn!("Nested level warn event {i}");
            error!("Nested level error event {i}");
            warn_span!("Nested Nested Warn Span").in_scope(|| {
                trace!("Nested Nested level trace event {i}");
                debug!("Nested Nested level debug event {i}");
                info!("Nested Nested level info event {i}");
                warn!("Nested Nested level warn event {i}");
                error!("Nested Nested level error event {i}");
            });
        })
    });
}

#[tokio::test]
async fn basic_spe_export_works() {
    std::env::set_var("TRACER_DEBUG", "true");
    let (_server_handle, mut receiver) = TestTracerCollector::new(4200, "/websocket/collector");
    let flush_handle = crate::setup_tracer_client_or_panic(TracerConfig {
        collector_url: "ws://127.0.0.1:4200/websocket/collector".to_string(),
        env: "test".to_string(),
        service_name: "test_service".to_string(),
        filters: TracerFilters {
            global: Level::Info,
            per_crate: Default::default(),
            per_span: Default::default(),
        },
        ws_timeout: Duration::from_secs(1),
        ping_interval: Duration::from_secs(1),
        sampler_limits: SamplerLimits {
            span_plus_event_per_minute_per_trace_limit: 50,
            orphan_events_per_minute_limit: 50,
        },
        maximum_spe_buffer: 100,
    })
    .await;
    let websocket = match receiver.recv().await {
        None => {
            panic!("No client connected and server is gone");
        }
        Some(client) => client,
    };
    let timeout = Duration::from_secs(1);
    let collector_processing_delay = Duration::from_secs(0);
    let (sender, mut received_messages) = tokio::sync::mpsc::channel(10);
    let (websocket_server_handle, mut _req_resp) =
        ReqResp::new_tracer_server(timeout, timeout, websocket, {
            let sender = sender.clone();
            Box::new(move |msg| Box::pin(handle(msg, sender.clone(), collector_processing_delay)))
        });
    let mut receiver = MessageExpecter {
        receiver: &mut received_messages,
        max_wait: Duration::from_millis(1),
    };
    let msg = "Some Info orphan message";
    info!("{}", msg);
    flush_handle.flush(timeout).await.unwrap();
    flush_handle.flush(timeout).await.unwrap();
    let orphan_event = receiver.expect_orphan_event().await;
    assert_eq!(orphan_event.name, msg);
    assert_eq!(orphan_event.level, Level::Info);
    error_span!("Error Span").in_scope(|| {
        trace!("Top level trace event");
        debug!("Top level debug event");
        info!("Top level info event");
        warn!("Top level warn event");
        error!("Top level error event");
        info_span!("Nested Info Span").in_scope(|| {
            warn!("Warn event");
            warn_span!("Nested Nested Warn Span").in_scope(|| {
                // yeah, thread sleep to simulate busy code
                std::thread::sleep(Duration::from_millis(100));
            });
        })
    });

    tokio::time::sleep(Duration::from_millis(110)).await;
    flush_handle.flush(timeout).await.unwrap();
    let millis_to_nanos = 1_000_000u64;
    let mut open_span_ids = vec![];
    let new_span = receiver.expect_new_span().await;
    open_span_ids.push(new_span.id);
    assert_eq!(new_span.name, "Error Span");
    let new_span_event = receiver.expect_new_span_event().await;
    assert_eq!(new_span_event.name, "Top level info event");
    assert_eq!(new_span_event.level, Level::Info);
    let new_span_event = receiver.expect_new_span_event().await;
    assert_eq!(new_span_event.name, "Top level warn event");
    assert_eq!(new_span_event.level, Level::Warn);
    let new_span_event = receiver.expect_new_span_event().await;
    assert_eq!(new_span_event.name, "Top level error event");
    assert_eq!(new_span_event.level, Level::Error);
    let new_span = receiver.expect_new_span().await;
    open_span_ids.push(new_span.id);
    assert_eq!(new_span.name, "Nested Info Span");
    let new_span_event = receiver.expect_new_span_event().await;
    assert_eq!(new_span_event.name, "Warn event");
    assert_eq!(new_span_event.level, Level::Warn);
    let new_span = receiver.expect_new_span().await;
    open_span_ids.push(new_span.id);
    assert_eq!(new_span.name, "Nested Nested Warn Span");
    let closed_span = receiver.expect_closed_span().await;
    assert_eq!(closed_span.id, open_span_ids.pop().unwrap());
    assert!(closed_span.duration > 100 * millis_to_nanos);
    assert!(closed_span.duration < 150 * millis_to_nanos);
    let closed_span = receiver.expect_closed_span().await;
    assert_eq!(closed_span.id, open_span_ids.pop().unwrap());
    assert!(closed_span.duration > 100 * millis_to_nanos);
    assert!(closed_span.duration < 150 * millis_to_nanos);
    let closed_span = receiver.expect_closed_span().await;
    assert_eq!(closed_span.id, open_span_ids.pop().unwrap());
    assert!(closed_span.duration > 100 * millis_to_nanos);
    assert!(closed_span.duration < 150 * millis_to_nanos);
    // shutdown_handle.shutdown().await.unwrap();
}

fn assert_is_orphan(msg: ApplicationToCollectorMessage, expected_name: &str, expected_lvl: Level) {
    let ApplicationToCollectorMessage::NewOrphanEvent(NewOrphanEvent{ name, timestamp, level, key_vals }) = msg else{
        panic!("got unexpected msg: {:#?}", msg);
    };
    assert_eq!(name, expected_name);
    assert_eq!(level, expected_lvl);
}
async fn assert_application_responds_to_stats_request(sender: &mut ReqResp) {
    let response = sender
        .send(Message::new(CollectorToApplicationMessage::GetStats))
        .await
        .unwrap()
        .await
        .unwrap();
    if let ApplicationToCollectorMessage::GetStatsResponse(config) = response.data {
        println!("Got stats back: {:#?}", config)
    } else {
        panic!("Did not get stats response :(")
    }
}

async fn assert_application_responds_to_config_request(sender: &mut ReqResp) {
    println!("Sending config request");
    let response = sender
        .send(Message::new(CollectorToApplicationMessage::GetConfig))
        .await
        .unwrap()
        .await
        .unwrap();
    if let ApplicationToCollectorMessage::GetConfigResponse(config) = response.data {
        println!("Got config back: {:#?}", config)
    } else {
        panic!("Did not get config response :(")
    }
}
