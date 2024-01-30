use crate::api::state::AppState;

use api_structs::instance::update::ProducerStats;
use api_structs::InstanceId;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use backtraced_error::error_to_pretty_formatted;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::task::JoinHandle;
use tracing::{error, info, instrument};

pub mod database;
pub mod handlers;
pub mod state;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveServiceInstance {
    pub id: InstanceId,
    pub last_seen_timestamp: u64,
    pub filters: String,
    pub tracer_stats: ProducerStats,
}

#[instrument(skip_all)]
pub fn start(app_state: AppState, api_port: u16) -> JoinHandle<()> {
    info!("Starting API, checking if index.html UI file exist");
    if std::fs::read("./tracer-ui/dist/index.html").is_err() {
        panic!("Failed to read ./tracer-ui/dist/index.html");
    }
    info!("it does");
    let serve_ui = tower_http::services::ServeDir::new("./tracer-ui/dist").fallback(
        tower_http::services::ServeFile::new("./tracer-ui/dist/index.html"),
    );

    // List, Overview and Manage Services
    let service_routes = axum::Router::new()
        .route(
            "/list",
            axum::routing::get(handlers::ui::service::ui_service_list_get),
        )
        .route(
            "/overview",
            axum::routing::get(handlers::ui::service::ui_service_overview_get),
        )
        .route(
            "/filter",
            axum::routing::post(handlers::ui::service::ui_service_filter_post),
        );
    let instance_routes = axum::Router::new()
        .route(
            "/connect",
            axum::routing::get(handlers::instance::connect::instance_connect_get),
        )
        .route(
            "/update",
            axum::routing::post(handlers::instance::update::instance_update_post),
        );
    let trace_routes = axum::Router::new()
        .route(
            "/grid",
            axum::routing::get(handlers::ui::trace::grid::ui_trace_grid_get),
        )
        .route(
            "/chunk/list",
            axum::routing::get(handlers::ui::trace::chunk::ui_trace_chunk_list_get),
        )
        .route(
            "/chunk",
            axum::routing::get(handlers::ui::trace::chunk::ui_trace_chunk_get),
        )
        .route(
            "/autocomplete",
            axum::routing::get(handlers::ui::trace::grid::ui_trace_autocomplete_get),
        );
    let app = axum::Router::new()
        .route("/api/ready", axum::routing::get(ready_get))
        .nest("/api/ui/service", service_routes)
        .nest("/api/instance", instance_routes)
        .nest("/api/ui/trace", trace_routes)
        .route(
            "/api/ui/orphan_events",
            axum::routing::get(handlers::ui::orphan_event::ui_orphan_events_get),
        )
        .with_state(app_state)
        .fallback_service(serve_ui)
        // 100 MB
        .layer(axum::extract::DefaultBodyLimit::max(104857600))
        .layer(tower_http::cors::CorsLayer::very_permissive());
    tokio::spawn(async move {
        axum::Server::bind(
            &format!("0.0.0.0:{}", api_port)
                .parse()
                .expect("should be able to api server desired address and port"),
        )
        .serve(app.into_make_service())
        .await
        .expect("http server launch to not fail")
    })
}

// fn clean_up_service_instances_task(live_instances: LiveInstances) -> JoinHandle<()> {
//     trace!("Starting clean_up_services_task");
//     spawn_local(async move {
//         tokio::time::sleep(Duration::from_secs(60)).await;
//         clean_up_service_instances(&live_instances);
//     })
// }

// #[instrument(skip_all)]
// fn clean_up_service_instances(live_instances: &LiveInstances) {
//     trace!("cleaning up service");
//     live_instances.see_handle.write().retain(|id, val| {
//         if val.is_closed() {
//             debug!("dropping sse_handle for instance with id: {id}");
//             false
//         } else {
//             true
//         }
//     });
//     live_instances
//         .trace_data
//         .write()
//         .retain(|service_name, instance_list| {
//             instance_list.retain(|single_instance| {
//                 let date = api_structs::time_conversion::time_from_nanos(
//                     single_instance.last_seen_timestamp,
//                 );
//                 let last_seen_as_secs = chrono::Utc::now()
//                     .naive_utc()
//                     .signed_duration_since(date)
//                     .num_seconds();
//                 if last_seen_as_secs > 12 * 60 * 60 {
//                     debug!(
//                         "dropping instance {} - id: {} - last seen {}s ago",
//                         single_instance.service_name, single_instance.instance_id, last_seen_as_secs
//                     );
//                     false
//                 } else {
//                     true
//                 }
//             });
//             if instance_list.is_empty(){
//                 debug!(
//                         "Last instance of {service_name} was dropped, removing it from list of services",
//                     );
//                 false
//             }else{
//                 true
//             }
//
//         });
// }

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RawGridErrorSample {
    span_name: String,
    span_attributes: HashMap<String, String>,
    event: String,
    event_attributes: HashMap<String, String>,
    event_timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GridErrorSample {
    span_name: String,
    span_attributes: HashMap<String, String>,
    event: String,
    event_attributes: HashMap<String, String>,
    event_timestamp_unix_ms: i64,
}

pub fn u64_nanos_to_db_i64(val: u64) -> Result<i64, ApiError> {
    let as_i64 = i64::try_from(val).map_err(|_| ApiError {
        code: StatusCode::BAD_REQUEST,
        message: "Invalid timestamp, doesnt fit into i64".to_string(),
    })?;
    Ok(as_i64)
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

impl From<backtraced_error::SqlxError> for ApiError {
    fn from(err: backtraced_error::SqlxError) -> Self {
        error!("{:?}", error_to_pretty_formatted(err));
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "DB error when handling the request".to_string(),
        }
    }
}

async fn ready_get() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=UTF-8",
        )],
        "ok".to_string(),
    )
}
