use std::collections::HashMap;
use std::net::SocketAddr;

use crate::api::state::AppState;
use api_structs::{Endpoint, InstanceGlobalId};
use axum::response::IntoResponse;
use axum::ServiceExt;
use chrono::NaiveDateTime;
use http::{Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::task::JoinHandle;
use tower::Layer;
use tracing::field::Empty;
use tracing::Span;
use tracing::{error, info, instrument};
use tracked_error::error_chain_to_pretty_formatted;
use valuable::Valuable;
pub mod database;
pub mod handlers;
pub mod state;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveServiceInstance {
    pub id: InstanceGlobalId,
    pub last_seen_timestamp: u64,
    pub filters: String,
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
        .route("/", axum::routing::get(handlers::ui::service::get))
        .route(
            "/filter",
            axum::routing::post(handlers::ui::service::ui_service_filter_post),
        );
    let instance_routes = axum::Router::new()
        .route(
            "/register",
            axum::routing::post(handlers::instance::register::handler),
        )
        .route(
            "/update",
            axum::routing::post(handlers::instance::update::handler),
        );
    let trace_routes = axum::Router::new()
        .route(
            "/grid",
            axum::routing::get(handlers::ui::trace::grid::ui_trace_grid_get),
        )
        .route(
            "/search",
            axum::routing::get(handlers::ui::trace::event_search::search),
        )
        .route(
            "/keys",
            axum::routing::post(handlers::ui::trace::event_search::trace_keys),
        )
        .route(
            "/autocomplete",
            axum::routing::get(handlers::ui::trace::grid::ui_trace_autocomplete_get),
        );

    let app = axum::Router::new()
        .route("/api/ready", axum::routing::get(ready_get))
        .route(
            <api_structs::ui::series::GetSeries as Endpoint>::PATH,
            axum::routing::get(handlers::ui::series::get_all_series),
        )
        .nest("/api/ui/service", service_routes)
        .nest("/api/instance", instance_routes)
        .route(
            api_structs::ui::trace::time_series::TraceSummary::PATH,
            axum::routing::get(handlers::ui::trace::time_series::handler),
        )
        .nest("/api/ui/trace", trace_routes)
        .with_state(app_state)
        .fallback_service(serve_ui)
        .layer(axum::extract::DefaultBodyLimit::max(104_857_600))
        .layer(tower_http::cors::CorsLayer::very_permissive())
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::decompression::RequestDecompressionLayer::new())
        .layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    let method = request.method();
                    let path = request.uri().path();
                    let header_name = request.headers();
                    let headers = header_name
                        .into_iter()
                        .map(|(name, value)| {
                            (
                                name.as_str(),
                                value.to_str().unwrap_or_else(|_e| "non-utf8 value"),
                            )
                        })
                        .collect::<HashMap<&str, &str>>();
                    tracing::error_span!(
                        "request",
                        http.request.method = %method,
                        url.path = path,
                        headers = headers.as_value(),
                        http.response.status_code = Empty
                    )
                })
                // .on_request(|request, _span: &Span| {
                // tracing::debug!("started {} {}", request, request.uri().path())
                // })
                .on_response(
                    |response: &Response<axum::body::Body>, _latency: Duration, span: &Span| {
                        let status_code = response.status().as_u16();
                        span.record("http.response.status_code", status_code);
                    },
                ), // .on_body_chunk(|chunk: &bytes::Bytes, latency, _span: &Span| {
                   //     tracing::debug!("sending {} bytes", chunk.len())
                   // })
                   // .on_eos(|trailers, stream_duration, _span: &Span| {
                   //     tracing::debug!("stream closed after {:?}", stream_duration)
                   // })
                   // .on_failure(
                   //     |error: ServerErrorsFailureClass, latency: Duration, _span: &Span| {
                   //         tracing::debug!("something went wrong")
                   //     },
                   // ),
        );
    let app = tower_http::normalize_path::NormalizePathLayer::trim_trailing_slash().layer(app);
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(
            &format!("0.0.0.0:{}", api_port)
                .parse::<SocketAddr>()
                .expect("should be able to api server desired address and port"),
        )
        .await
        .unwrap();
        axum::serve(
            listener,
            ServiceExt::<axum::extract::Request>::into_make_service(app),
        )
        .await
        .expect("http server launch to not fail")
    })
}

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

#[allow(unused)]
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
    fn into_response(self) -> axum::response::Response {
        (self.code, self.message).into_response()
    }
}

impl From<tracked_error::SqlxError> for ApiError {
    fn from(err: tracked_error::SqlxError) -> Self {
        error!("{:?}", error_chain_to_pretty_formatted(err));
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "DB error when handling the request".to_string(),
        }
    }
}

impl From<tracked_error::SerdeJsonError> for ApiError {
    fn from(err: tracked_error::SerdeJsonError) -> Self {
        error!("{:?}", error_chain_to_pretty_formatted(err));
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Serialization error when handling the request".to_string(),
        }
    }
}

impl From<crate::error::EdgeDBOrSerdeJson> for ApiError {
    fn from(err: crate::error::EdgeDBOrSerdeJson) -> Self {
        error!("{}", error_chain_to_pretty_formatted(err));
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Database or Serde error when handling the request".to_string(),
        }
    }
}

impl From<tracked_error::EdgeDBError> for ApiError {
    fn from(err: tracked_error::EdgeDBError) -> Self {
        error!("{:?}", error_chain_to_pretty_formatted(err));
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "EdgeDB error when handling the request".to_string(),
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
