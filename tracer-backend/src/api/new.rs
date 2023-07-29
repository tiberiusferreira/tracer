use crate::api::{ApiError, AppState, InstanceStatusStorage, TracerClientInfo, UpdateFilter};
use crate::otel_trace_processing;
use crate::otel_trace_processing::{DbEvent, DbSpan};
use api_structs::exporter::{
    ApplicationToCollectorMessage, CollectorToApplicationMessage, Config, InstanceStatus, Level,
    TraceStats, TracerFilters, TracerStats,
};
use api_structs::websocket;
use api_structs::websocket::axum_adapter::SenderWsSink;
use api_structs::websocket::{RequestResponse, WsClient};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::{Error, Json, RequestExt};
use chrono::{NaiveDateTime, Timelike};
use futures::pin_mut;
use futures_concurrency::stream::IntoStream;
use futures_concurrency::stream::StreamExt;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt as StreamExt1};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::pin::{pin, Pin};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::oneshot::Receiver;
use tokio::time::timeout;
use tokio_stream::wrappers::ReceiverStream;
use tracing::instrument;

#[axum::debug_handler]
#[instrument(skip_all)]
pub(crate) async fn get_instances(
    State(instance_status_storage): State<InstanceStatusStorage>,
) -> Result<Json<Vec<InstanceStatus>>, ApiError> {
    let instances: HashMap<String, TracerClientInfo> = instance_status_storage.0.read().clone();
    let instances_status: Vec<InstanceStatus> = instances.into_values().map(|i| i.status).collect();
    Ok(Json(instances_status))
}

#[axum::debug_handler]
#[instrument(skip_all)]
pub(crate) async fn update_filters(
    State(instance_status_storage): State<InstanceStatusStorage>,
    Json(new_filter): Json<api_structs::exporter::ChangeTracerFiltersRequest>,
) -> Result<(), ApiError> {
    let instance_not_found = ApiError {
        code: StatusCode::OK,
        message: "Instance no longer exists".to_string(),
    };
    let ws_error = |e: String| ApiError {
        code: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!(
            "Error sending request to instance, maybe it has gone away: {}",
            e
        ),
    };
    let instance: Option<TracerClientInfo> = {
        let r_guard = instance_status_storage.0.read();
        let instance = r_guard.get(&new_filter.uuid).cloned();
        instance
    };
    return match instance {
        None => Err(instance_not_found),
        Some(instance) => match instance.ws.upgrade() {
            None => Err(instance_not_found),
            Some(instance_ws) => {
                let instance_ws: Arc<
                    tokio::sync::RwLock<
                        WsClient<
                            CollectorToApplicationMessage,
                            ApplicationToCollectorMessage,
                            Pin<Box<SenderWsSink>>,
                        >,
                    >,
                > = instance_ws;
                let mut w_instance_ws = instance_ws.write().await;
                let response = w_instance_ws
                    .send_msg_and_wait_for_response(websocket::Message::new(
                        CollectorToApplicationMessage::ChangeFilters(new_filter.new_trace_filters),
                    ))
                    .await;
                match response {
                    Ok(mut receiver) => match receiver.recv().await {
                        None => Err(ws_error("Got no response back from instance".to_string())),
                        Some(response) => {
                            let websocket::Message {
                                data: ApplicationToCollectorMessage::ChangeFiltersResponse(resp),
                                ..
                            } = response else{
                                return Err(ApiError {
                                    code: StatusCode::INTERNAL_SERVER_ERROR,
                                    message: format!(
                                        "Instance sent bad response: {:#?}",
                                        response
                                    ),
                                });
                            };
                            match resp {
                                Ok(new_filter) => {
                                    println!("new filter: {}", new_filter);
                                    Ok(())
                                }
                                Err(error) => Err(ApiError {
                                    code: StatusCode::INTERNAL_SERVER_ERROR,
                                    message: format!("Instance sent bad response: {:#?}", error),
                                }),
                            }
                        }
                    },
                    Err(e) => Err(ws_error(format!("{:?}", e))),
                }
            }
        },
    };
}
#[axum::debug_handler]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(instance_status_storage): State<InstanceStatusStorage>,
) -> impl axum::response::IntoResponse {
    // finalize the upgrade process by returning upgrade callback.
    // we can customize the callback by sending additional info such as address.
    ws.on_upgrade(move |socket| tracer_client_loop(socket, instance_status_storage))
}

async fn send_msg(
    socket: &mut SplitSink<WebSocket, Message>,
    msg: CollectorToApplicationMessage,
) -> Result<(), ()> {
    if let Err(e) = socket
        .send(Message::Text(serde_json::to_string(&msg).unwrap()))
        .await
    {
        println!("{:#?}", e);
        return Err(());
    }
    Ok(())
}

fn parse_msg(msg: Result<Message, Error>) -> Result<ApplicationToCollectorMessage, ()> {
    return match msg {
        Ok(msg) => match msg {
            Message::Text(msg) => match serde_json::from_str(&msg) {
                Ok(a) => Ok(a),
                Err(e) => {
                    println!("{:#?}", e);
                    Err(())
                }
            },
            _ => Err(()),
        },
        Err(e) => {
            println!("{:?}", e);
            Err(())
        }
    };
}
async fn get_msg(socket: &mut SplitStream<WebSocket>) -> Result<ApplicationToCollectorMessage, ()> {
    while let Some(msg) = socket.next().await {
        return parse_msg(msg);
    }
    Err(())
}

async fn get_config(socket: &mut SplitStream<WebSocket>) -> Result<Config, ()> {
    match get_msg(socket).await {
        Ok(msg) => {
            return if let ApplicationToCollectorMessage::GetConfigResponse(config) = msg {
                Ok(config)
            } else {
                Err(())
            }
        }
        Err(e) => {
            return Err(());
        }
    }
}

// async fn handle_msg(
//     msg: ApplicationToCollectorMessages,
//     config: &Config,
//     instance_status_storage: &InstanceStatusStorage,
// ) {
//     match msg {
//         ApplicationToCollectorMessages::GetConfigResponse(_) => {
//             return;
//         }
//         ApplicationToCollectorMessages::GetStorageStatusResponse(new_storage_status) => {
//             let mut w_instance_status_storage = instance_status_storage.0.write();
//             if let Some((instance_status, sender)) = w_instance_status_storage.get_mut(&config.uuid)
//             {
//                 // instance_status.dropped_spe = new_storage_status.dropped_traces;
//                 for (trace_name, per_minute_spe_usage) in new_storage_status.per_minute_spe_usage {
//                     let entry =
//                         instance_status
//                             .stats_per_trace
//                             .entry(trace_name)
//                             .or_insert(TraceStats {
//                                 warnings: 0,
//                                 errors: 0,
//                                 per_minute_spe_usage,
//                             });
//                     entry.per_minute_spe_usage = per_minute_spe_usage;
//                 }
//             }
//         }
//         ApplicationToCollectorMessages::NewSpan(_) => {}
//         ApplicationToCollectorMessages::NewSpanEvent(_) => {}
//         ApplicationToCollectorMessages::ClosedSpan(_) => {}
//         ApplicationToCollectorMessages::NewOrphanEvent(_) => {}
//         ApplicationToCollectorMessages::ChangeFiltersResponse(_) => {}
//     }
// }

async fn request_and_get_config(
    sender: &mut SplitSink<WebSocket, Message>,
    receiver: &mut SplitStream<WebSocket>,
) -> Option<Config> {
    let Ok(()) = send_msg(sender, CollectorToApplicationMessage::GetConfig).await else{
        return None;
    };
    let Ok(config) = get_config(receiver).await else{
        return None;
    };
    Some(config)
}

struct GetStorageStatusRequest {
    sender: Option<tokio::sync::oneshot::Sender<TracerStats>>,
    task: tokio::task::JoinHandle<()>,
}
#[derive(Debug)]
pub enum RunningRequests {
    GetStorageStatus(
        (
            Option<tokio::sync::oneshot::Sender<TracerStats>>,
            tokio::task::JoinHandle<()>,
        ),
    ),
}

async fn storage_status_updater(
    mut ws_sender: Arc<tokio::sync::RwLock<SplitSink<WebSocket, Message>>>,
    mut storage_status_receiver: tokio::sync::mpsc::Receiver<TracerStats>,
    instance_status_storage: InstanceStatusStorage,
) {
    loop {
        let mut ws_sender = ws_sender.write().await;
        if let Ok(()) = send_msg(&mut ws_sender, CollectorToApplicationMessage::GetStats).await {
            drop(ws_sender);
            if let Ok(Some(storage_status)) =
                timeout(Duration::from_secs(5), storage_status_receiver.recv()).await
            {
                let w_guard = instance_status_storage.0.write();
                // update storage
                println!("Updated storage using: {:#?}", storage_status);
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

async fn get_tracer_client_config(
    sender: &mut WsClient<
        CollectorToApplicationMessage,
        ApplicationToCollectorMessage,
        Pin<Box<SenderWsSink>>,
    >,
) -> Result<Config, ()> {
    let mut config_resp = sender
        .send_msg_and_wait_for_response(api_structs::websocket::Message::new(
            CollectorToApplicationMessage::GetConfig,
        ))
        .await
        .unwrap();
    if let Some(websocket::Message {
        data: ApplicationToCollectorMessage::GetConfigResponse(config),
        ..
    }) = config_resp.recv().await
    {
        Ok(config)
    } else {
        Err(())
    }
}

async fn tracer_client_loop(socket: WebSocket, instance_status_storage: InstanceStatusStorage) {
    println!("New TracerClient!");
    let (mut sender, mut receiver) = websocket::axum_adapter::setup(socket).await;
    println!("Getting config!");
    let Ok(config) = get_tracer_client_config(&mut sender).await else{
        println!("Got no config!");
        return;
    };
    println!("Got NEW config!");

    let uuid = config.uuid.clone();
    let sender = Arc::new(tokio::sync::RwLock::new(sender));
    {
        let mut w_instance_status_storage = instance_status_storage.0.write();
        let instance = w_instance_status_storage
            .entry(config.uuid.clone())
            .or_insert(TracerClientInfo {
                status: InstanceStatus {
                    config: config.clone(),
                    spe_dropped_on_export: 0,
                    orphan_events_per_minute_usage: 0,
                    orphan_events_per_minute_dropped: 0,
                    trace_stats: HashMap::new(),
                },
                ws: Arc::downgrade(&sender),
            });
        instance.status.config = config;
    }
    println!("Added instance!");
    let update_storate_status_task = tokio::task::spawn(async move {
        loop {
            let mut sender = sender.write().await;

            let resp = sender
                .send_msg_and_wait_for_response(websocket::Message::new(
                    CollectorToApplicationMessage::GetStats,
                ))
                .await;
            match resp {
                Ok(mut resp) => match resp.recv().await {
                    None => {
                        println!("Instance died");
                    }
                    Some(resp) => match resp.data {
                        ApplicationToCollectorMessage::GetStatsResponse(resp) => {
                            let mut w_instance_status = instance_status_storage.0.write();
                            let instance_status = w_instance_status.get_mut(&uuid);
                            match instance_status {
                                None => {
                                    println!("Instance status no longer exists, bug");
                                }
                                Some(status) => {
                                    status.status.orphan_events_per_minute_dropped =
                                        resp.orphan_events_per_minute_dropped;
                                    status.status.orphan_events_per_minute_usage =
                                        resp.orphan_events_per_minute_usage;
                                    status.status.spe_dropped_on_export =
                                        resp.spe_dropped_on_export;
                                    for (trace, per_minute_trace_stats) in
                                        resp.per_minute_trace_stats
                                    {
                                        let entry =
                                            status.status.trace_stats.entry(trace).or_insert(
                                                TraceStats {
                                                    warnings: 0,
                                                    errors: 0,
                                                    spe_usage_per_minute: per_minute_trace_stats
                                                        .spe_usage_per_minute,
                                                    dropped_per_minute: per_minute_trace_stats
                                                        .dropped_per_minute,
                                                },
                                            );
                                        entry.spe_usage_per_minute =
                                            per_minute_trace_stats.spe_usage_per_minute;
                                        entry.dropped_per_minute =
                                            per_minute_trace_stats.dropped_per_minute;
                                    }
                                }
                            }
                        }
                        x => {
                            println!("Unexpected response: {:#?}", x);
                        }
                    },
                },
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    while let Some(msg) = receiver.recv().await {
        match &msg.data {
            ApplicationToCollectorMessage::NewSpan(span) => {
                println!("New span: {:#?}", span);
            }
            ApplicationToCollectorMessage::NewSpanEvent(event) => {
                println!("New event: {:#?}", event);
            }
            ApplicationToCollectorMessage::ClosedSpan(closed_span) => {
                println!("Closed Span: {:#?}", closed_span);
            }
            ApplicationToCollectorMessage::NewOrphanEvent(new_orphan_event) => {
                println!("New orphan event: {:#?}", new_orphan_event);
            }
            x => {
                println!("Unexpected msg: {:#?}", x);
            }
        }
    }
    println!("Receiver closed! Closing loop!");
    // TODO: fix
    update_storate_status_task.abort();
    println!("Closing connection!");
}

// #[instrument(skip_all)]
// pub(crate) async fn post_single_trace(
//     axum::extract::State(con): axum::extract::State<PgPool>,
//     trace: Json<api_structs::exporter::Trace>,
// ) -> Result<(), ApiError> {
//     println!("New trace!");
//     let trace = trace.0;
//     let mut span_id_to_idx =
//         trace
//             .children
//             .iter()
//             .enumerate()
//             .fold(HashMap::new(), |mut acc, (idx, curr)| {
//                 acc.insert(curr.id, idx + 2);
//                 acc
//             });
//     span_id_to_idx.insert(trace.id, 1);
//     let mut spans = vec![];
//     let mut root_events = vec![];
//     for (idx, e) in trace.events.into_iter().enumerate() {
//         root_events.push(DbEvent {
//             id: i64::try_from(idx + 1).expect("idx to fit i64"),
//             timestamp: i64::try_from(e.timestamp).expect("timestamp to fit i64"),
//             name: e.name,
//             key_values: vec![],
//             severity: otel_trace_processing::Level::Info,
//         });
//     }
//     spans.push(DbSpan {
//         id: 1,
//         timestamp: i64::try_from(trace.start).expect("timestamp to fit i64"),
//         parent_id: None,
//         name: trace.name.clone(),
//         duration: i64::try_from(trace.duration).expect("timestamp to fit i64"),
//         key_values: vec![],
//         events: root_events,
//     });
//     for span in trace.children.into_iter() {
//         let mut events = vec![];
//         for (idx, e) in span.events.into_iter().enumerate() {
//             events.push(DbEvent {
//                 id: i64::try_from(idx + 1).expect("idx to fit i64"),
//                 timestamp: i64::try_from(e.timestamp).expect("timestamp to fit i64"),
//                 name: e.name,
//                 key_values: vec![],
//                 severity: otel_trace_processing::Level::Info,
//             });
//         }
//         spans.push(DbSpan {
//             id: i64::try_from(*span_id_to_idx.get(&span.id).expect("span id to exist"))
//                 .expect("usize to fit i64"),
//             timestamp: i64::try_from(span.start).expect("timestamp to fit i64"),
//             parent_id: Some(
//                 i64::try_from(
//                     *span_id_to_idx
//                         .get(&span.parent_id)
//                         .expect("parent id to exist"),
//                 )
//                 .expect("parent id to fit i64"),
//             ),
//             name: span.name,
//             duration: i64::try_from(span.duration).expect("duration to fit i64"),
//             key_values: vec![],
//             events,
//         });
//     }
//     let data = crate::otel_trace_processing::DbReadyTraceData {
//         timestamp: i64::try_from(trace.start).expect("timestamp to fit i64"),
//         service_name: trace.service_name,
//         duration: i64::try_from(trace.duration).expect("duration to fit i64"),
//         top_level_span_name: trace.name,
//         has_errors: false,
//         warning_count: 0,
//         spans,
//         span_plus_events_count: 0,
//     };
//     match otel_trace_processing::store_trace(con, data).await {
//         Ok(id) => {
//             println!("Inserted: {}", id);
//         }
//         Err(e) => {
//             println!("{:#?}", e);
//         }
//     }
//     Ok(())
// }
