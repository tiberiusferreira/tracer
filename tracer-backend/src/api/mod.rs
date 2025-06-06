use crate::api::state::AppState;
use axum::response::IntoResponse;
use axum::{Router, ServiceExt};
use http::{Method, StatusCode};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::ops::DerefMut;
use std::sync::RwLock;
use tokio::task::JoinHandle;
use tracing_config_helper::io_provider::execution_recorder::record_single_attribute;
use tracked_error::error_chain_to_pretty_formatted;

pub mod handlers;
pub mod state;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct MyRequest {
    parts: MyParts,
    body: Vec<u8>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum MyMethod {
    Options,
    Get,
    Post,
    Put,
    Delete,
    Head,
    Trace,
    Connect,
    Patch,
}

impl Display for MyMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MyMethod::Options => f.write_str("options"),
            MyMethod::Get => f.write_str("get"),
            MyMethod::Post => f.write_str("post"),
            MyMethod::Put => f.write_str("put"),
            MyMethod::Delete => f.write_str("delete"),
            MyMethod::Head => f.write_str("head"),
            MyMethod::Trace => f.write_str("trace"),
            MyMethod::Connect => f.write_str("connect"),
            MyMethod::Patch => f.write_str("patch"),
        }
    }
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct MyParts {
    pub method: MyMethod,
    pub uri: String,
    pub headers: HashMap<String, String>,
}

fn my_request_to_axum(request: MyRequest) -> axum::extract::Request {
    let builder = http::request::Builder::new();
    let method = match request.parts.method {
        MyMethod::Options => &Method::OPTIONS,
        MyMethod::Get => &Method::GET,
        MyMethod::Post => &Method::POST,
        MyMethod::Put => &Method::PUT,
        MyMethod::Delete => &Method::DELETE,
        MyMethod::Head => &Method::HEAD,
        MyMethod::Trace => &Method::TRACE,
        MyMethod::Connect => &Method::CONNECT,
        MyMethod::Patch => &Method::PATCH,
    };
    let mut builder = builder.uri(request.parts.uri).method(method);
    for (k, v) in &request.parts.headers {
        builder = builder.header(k.to_string(), v.to_string());
    }

    let axum_body = axum::body::Body::new(axum::body::Body::from(request.body));
    let w = builder.body(axum_body).unwrap();
    w
}
async fn axum_request_to_serializable(request: axum::extract::Request) -> MyRequest {
    let uri = request.uri().to_string();
    let method = match request.method() {
        &Method::OPTIONS => MyMethod::Options,
        &Method::GET => MyMethod::Get,
        &Method::POST => MyMethod::Post,
        &Method::PUT => MyMethod::Put,
        &Method::DELETE => MyMethod::Delete,
        &Method::HEAD => MyMethod::Head,
        &Method::TRACE => MyMethod::Trace,
        &Method::CONNECT => MyMethod::Connect,
        &Method::PATCH => MyMethod::Patch,
        _ => panic!("{}", request.method()),
    };

    let headers: HashMap<String, String> = request
        .headers()
        .clone()
        .into_iter()
        .filter_map(|(k, v)| Some((k?.to_string(), v.to_str().unwrap().to_string())))
        .collect();
    let body_bytes = axum::body::to_bytes(request.into_body(), 100_000_000)
        .await
        .unwrap()
        .to_vec();
    MyRequest {
        parts: MyParts {
            method,
            uri,
            headers,
        },
        body: body_bytes,
    }
}

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
        let size_kb = my_request.body.len() / 1000;
        println!("Got self request of size {size_kb}kb",);
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
    let response = tracing_config_helper::io_provider::execution_recorder::record_execution(
        my_request,
        |my_request| async {
            let uri = my_request.parts.uri.clone();
            record_single_attribute("uri".to_string(), uri);
            record_single_attribute("method".to_string(), my_request.parts.method.to_string());
            let axum_req = my_request_to_axum(my_request);
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
            "/execution_list",
            axum::routing::post(handlers::ui::service::execution_list),
        )
        .route(
            "/execution",
            axum::routing::get(handlers::ui::execution::get_single_execution),
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
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=UTF-8",
        )],
        "ok".to_string(),
    )
}
