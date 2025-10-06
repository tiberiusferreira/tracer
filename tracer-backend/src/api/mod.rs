use crate::api::handlers::instance::update::ProcessUpdateError;
use crate::api::state::AppState;
use axum::response::IntoResponse;
use axum::{Router, ServiceExt};
use http::{StatusCode};
use std::net::SocketAddr;
use std::ops::DerefMut;
use std::sync::RwLock;
use tokio::task::JoinHandle;
use axum_adapter::{axum_request_to_serializable, recorded_request_to_axum, RecordedRequest};
use tracer::io_provider::execution_recorder::record_single_attribute;
use tracer::io_provider::is_playing_recording;
use tracked_error::error_chain_to_pretty_formatted;

pub mod handlers;
pub mod state;


static SELF_TRACE_SKIPPED_IN_SEQUENCE_COUNT: RwLock<u8> = RwLock::new(0);

async fn my_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let my_request = axum_request_to_serializable(request).await;
    let recording_enabled = if my_request.parts.uri == "/api/instance/update"
        && my_request
        .parts
        .headers
        .get("service-name")
        .is_some_and(|service_name| service_name == "tracer-backend")
    {
        let size_kb = my_request.body_base64.len() / 1000;
        println!("Got self request of size {size_kb}kb", );
        let mut w_guard = SELF_TRACE_SKIPPED_IN_SEQUENCE_COUNT.write().unwrap();
        let count = w_guard.deref_mut();
        if *count >= 3 && size_kb <= 1_000 {
            println!("keeping");
            *count = 0;
            true
        } else {
            println!("skipping");
            *count += 1;
            false
        }
    } else {
        true
    };
    if is_playing_recording() {
        let axum_req = recorded_request_to_axum(my_request);
        let resp = next.run(axum_req).await;
        return resp;
    }
    let response = tracer::io_provider::execution_recorder::record_execution(
        my_request,
        |my_request: RecordedRequest| async {
            let uri = my_request.parts.uri.clone();
            record_single_attribute("uri".to_string(), uri);
            record_single_attribute("method".to_string(), my_request.parts.method.to_string());
            let axum_req = recorded_request_to_axum(my_request);
            let resp = next.run(axum_req).await;
            let status = resp.status();
            record_single_attribute("status_code".to_string(), status.as_u16().to_string());
            resp
        },
        recording_enabled,
    )
        .await;
    response
}

pub fn create_router(app_state: AppState) -> Router<()> {
    println!("Starting API, checking if index.html UI file exist");
    if std::fs::read("/Users/tiberiodarferreira/Documents/github/tracer/tracer-ui/dist/index.html")
        .is_err()
    {
        panic!("Failed to read ./tracer-ui/dist/index.html");
    }
    let serve_ui = tower_http::services::ServeDir::new(
        "/Users/tiberiodarferreira/Documents/github/tracer/tracer-ui/dist",
    )
        .fallback(tower_http::services::ServeFile::new(
            "/Users/tiberiodarferreira/Documents/github/tracer/tracer-ui/dist/index.html",
        ));
    let service_routes = axum::Router::new()
        .route(
            "/data",
            axum::routing::post(handlers::ui::service::summaries_for_graph),
        )
        .route(
            "/instance-profile",
            axum::routing::get(handlers::ui::service::instance_profile),
        )
        .route(
            "/execution_list",
            axum::routing::post(handlers::ui::service::execution_list),
        )
        .route(
            "/execution",
            axum::routing::get(handlers::ui::execution_details::get_single_execution),
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
    let app = Router::new()
        .route("/api/ready", axum::routing::get(ready_get))
        .nest("/api/ui/service", service_routes)
        .nest("/api/instance", instance_routes)
        .with_state(app_state)
        .fallback_service(serve_ui)
        .layer(axum::extract::DefaultBodyLimit::max(50_000_000))
        .layer(axum::middleware::from_fn(my_middleware))
        .layer(tower_http::cors::CorsLayer::very_permissive())
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::decompression::RequestDecompressionLayer::new());
    app
}
pub fn start(app_state: AppState, api_port: u16) -> JoinHandle<()> {
    // List, Overview and Manage Services
    let app = create_router(app_state);
    tokio::spawn(async move {
        if is_playing_recording() {
            panic!("Should not be running in playback mode and get here");
        }
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

#[tokio::test]
async fn replay_api_recording() {
    dotenvy::dotenv().ok();
    unsafe { std::env::set_var("GLOBAL_RECORDING_PATH", "/Users/tiberiodarferreira/Documents/github/tracer/rec"); }
    use tower_service::Service;
    let app_state = AppState {
        execution_io_provider: gel_io_recorder::DatabaseIoRecorder::from_global_recording(),
    };
    let mut app = create_router(app_state);
    let resp = tracer::io_provider::execution_recorder::play_global_recording(move |request: RecordedRequest| async move {
        let axum_request = recorded_request_to_axum(request);
        app.call(axum_request).await.unwrap()
    }).await;
    let (parts, body) = resp.into_parts();
    let body_bytes = axum::body::to_bytes(body, 100_000_000)
        .await
        .unwrap()
        .to_vec();
    let text = String::from_utf8(body_bytes).unwrap();
    println!("Body:\n{}", text);
    println!("{:#?}", parts);
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

impl From<tracked_error::SerdeJsonError> for ApiError {
    fn from(_err: tracked_error::SerdeJsonError) -> Self {
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Serialization error when handling the request".to_string(),
        }
    }
}

impl From<gel_io_recorder::Error> for ApiError {
    fn from(err: gel_io_recorder::Error) -> Self {
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: error_chain_to_pretty_formatted(&err),
        }
    }
}

impl From<ProcessUpdateError> for ApiError {
    fn from(err: ProcessUpdateError) -> Self {
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: error_chain_to_pretty_formatted(&err),
        }
    }
}

impl From<gel_tokio::Error> for ApiError {
    #[track_caller]
    fn from(err: gel_tokio::Error) -> Self {
        let _tracked = tracked_error::TrackedError::from(err);
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "GelDB error when handling the request".to_string(),
        }
    }
}

impl From<tracked_error::EdgeDBError> for ApiError {
    fn from(_err: tracked_error::EdgeDBError) -> Self {
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "EdgeDB error when handling the request".to_string(),
        }
    }
}

async fn ready_get() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(http::header::CONTENT_TYPE, "text/plain; charset=UTF-8")],
        "ok".to_string(),
    )
}
