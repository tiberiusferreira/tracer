use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;

use crate::api::state::AppState;
use api_structs::instance::update::ReplayData;
use api_structs::{Endpoint, InstanceGlobalId};
use axum::response::IntoResponse;
use axum::{Router, ServiceExt};
use chrono::NaiveDateTime;
use http::{Method, Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::task::JoinHandle;
use tower::{Layer, Service};
use tower_http::normalize_path::NormalizePath;
use tracing::Span;
use tracing::field::Empty;
use tracing::{error, info, instrument};
use tracing_config_helper::io_provider::execution_recorder::{
    DataCollector, GLOBAL_DATA_COLLECTOR, record_single_attribute,
};
use tracing_config_helper::io_provider::{DatabaseIoProvider, ExecutionIoProvider};
use tracked_error::error_chain_to_pretty_formatted;
use valuable::Valuable;

pub mod handlers;
pub mod state;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveServiceInstance {
    pub id: InstanceGlobalId,
    pub last_seen_timestamp: u64,
    pub filters: String,
}

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
async fn my_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Service A -> Tracer update endpoint -> generates execution recording -> Tracer self-tracing endpoint
    // Service B -> Tracer update endpoint
    // Tracer -> Tracer self-tracing endpoint that records the data without generating a new execution recording
    let my_request = axum_request_to_serializable(request).await;
    let recording_enabled = if my_request.parts.uri == "/api/instance/update"
        && my_request
            .parts
            .headers
            .get("host")
            .is_some_and(|h| h == "127.0.0.1:4200")
    {
        // about to store data from ourselves
        false
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

pub fn create_router(app_state: AppState) -> NormalizePath<Router<()>> {
    info!("Starting API, checking if index.html UI file exist");
    if std::fs::read("/Users/tiberiodarferreira/Documents/github/tracer/tracer-ui/dist/index.html")
        .is_err()
    {
        panic!("Failed to read ./tracer-ui/dist/index.html");
    }
    info!("it does");
    let serve_ui = tower_http::services::ServeDir::new(
        "/Users/tiberiodarferreira/Documents/github/tracer/tracer-ui/dist",
    )
    .fallback(tower_http::services::ServeFile::new(
        "/Users/tiberiodarferreira/Documents/github/tracer/tracer-ui/dist/index.html",
    ));
    let service_routes = axum::Router::new()
        .route("/data", axum::routing::post(handlers::ui::service::data))
        .route(
            "/execution_list",
            axum::routing::post(handlers::ui::service::execution_list),
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
            "/search",
            axum::routing::get(handlers::ui::trace::event_search::search),
        )
        .route(
            "/keys",
            axum::routing::post(handlers::ui::trace::event_search::trace_keys),
        );
    GLOBAL_DATA_COLLECTOR.set(DataCollector::new()).unwrap();
    let app = axum::Router::new()
        .route("/api/ready", axum::routing::get(ready_get))
        .route(
            <api_structs::ui::series::GetSeries as Endpoint>::PATH,
            axum::routing::get(handlers::ui::series::get_all_series),
        )
        .nest("/api/ui/service", service_routes)
        .nest("/api/instance", instance_routes)
        .nest("/api/ui/trace", trace_routes)
        .with_state(app_state)
        .fallback_service(serve_ui)
        .layer(axum::middleware::from_fn(my_middleware))
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
                .on_response(
                    |response: &Response<axum::body::Body>, _latency: Duration, span: &Span| {
                        let status_code = response.status().as_u16();
                        span.record("http.response.status_code", status_code);
                    },
                ),
        );
    let app = tower_http::normalize_path::NormalizePathLayer::trim_trailing_slash().layer(app);
    app
}
#[instrument(skip_all)]
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

#[tokio::test]
async fn a() {
    let replay_data: ReplayData = serde_json::from_str(REPLAY_DATA).unwrap();
    let req: MyRequest = serde_json::from_value(replay_data.input).unwrap();
    let edgedb_client = gel_tokio::create_client().await.unwrap();
    let app_state = AppState {
        gel_client: edgedb_client.clone(),
        execution_io_provider: ExecutionIoProvider {
            database: DatabaseIoProvider::Live(edgedb_client),
        },
    };
    let mut app = create_router(app_state);
    let axum_req = my_request_to_axum(req);
    println!("{:?}", axum_req);
    let resp = app.call(axum_req);
    let w = resp.await.unwrap();
    println!("{:?}", w);
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

impl From<tracked_error::SerdeJsonError> for ApiError {
    fn from(err: tracked_error::SerdeJsonError) -> Self {
        error!("{:?}", error_chain_to_pretty_formatted(err));
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Serialization error when handling the request".to_string(),
        }
    }
}

impl From<api_structs::instance::update::Error> for ApiError {
    fn from(err: api_structs::instance::update::Error) -> Self {
        error!("{:?}", error_chain_to_pretty_formatted(&err));
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: error_chain_to_pretty_formatted(&err),
        }
    }
}

impl From<gel_tokio::Error> for ApiError {
    #[track_caller]
    fn from(err: gel_tokio::Error) -> Self {
        let tracked = tracked_error::TrackedError::from(err);
        error!("{:?}", error_chain_to_pretty_formatted(tracked));
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "GelDB error when handling the request".to_string(),
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

const REPLAY_DATA: &str = r#"{
  "input": {
    "body": [
      123,
      34,
      110,
      97,
      109,
      101,
      34,
      58,
      34,
      116,
      114,
      97,
      99,
      101,
      114,
      45,
      98,
      97,
      99,
      107,
      101,
      110,
      100,
      34,
      44,
      34,
      101,
      110,
      118,
      34,
      58,
      34,
      108,
      111,
      99,
      97,
      108,
      34,
      125
    ],
    "parts": {
      "uri": "/api/instance/register",
      "method": "Post",
      "headers": {
        "host": "127.0.0.1:4200",
        "accept": "*/*",
        "content-type": "application/json",
        "content-length": "39",
        "accept-encoding": "br"
      }
    }
  },
  "database_recording": {
    "transactions": [
      {
        "id": 0,
        "result": {
          "result": {
            "Ok": null
          },
          "ended_at": "2025-05-11T07:24:29.054351Z"
        },
        "queries": [
          {
            "id": 0,
            "result": {
              "result": {
                "Ok": {
                  "id": "eeea0606-2e27-11f0-bb39-972ef7ea354b"
                }
              },
              "ended_at": "2025-05-11T07:24:29.026296Z"
            },
            "started_at": "2025-05-11T07:24:29.010125Z",
            "query_with_parameters": {
              "parameters": {
                "env": {
                  "String": "local"
                },
                "service": {
                  "String": "tracer-backend"
                }
              },
              "query_text": "with\n    env := <str>$env,\n    name := <str>$service,\nselect Service{\n  id\n} filter .env = env and .name = name;"
            }
          },
          {
            "id": 1,
            "result": {
              "result": {
                "Ok": {
                  "id": "f9aea7c0-2e38-11f0-8373-574a6aaedde4"
                }
              },
              "ended_at": "2025-05-11T07:24:29.035577Z"
            },
            "started_at": "2025-05-11T07:24:29.026351Z",
            "query_with_parameters": {
              "parameters": {
                "service": {
                  "Uuid": {
                    "val": "eeea0606-2e27-11f0-bb39-972ef7ea354b",
                    "cast_to_table": "Service"
                  }
                }
              },
              "query_text": "insert ServiceInstance{\n    service := <Service><uuid>$service\n}"
            }
          },
          {
            "id": 2,
            "result": {
              "result": {
                "Ok": {
                  "id": "f9afcb96-2e38-11f0-8373-0f91737742a5"
                }
              },
              "ended_at": "2025-05-11T07:24:29.042822Z"
            },
            "started_at": "2025-05-11T07:24:29.035632Z",
            "query_with_parameters": {
              "parameters": {
                "new": {
                  "Json": {
                    "service": "eeea0606-2e27-11f0-bb39-972ef7ea354b"
                  }
                },
                "entity_id": {
                  "Uuid": {
                    "val": "f9aea7c0-2e38-11f0-8373-574a6aaedde4",
                    "cast_to_table": null
                  }
                },
                "entity_name": {
                  "String": "ServiceInstance"
                },
                "execution_id": {
                  "Uuid": {
                    "val": "c8b100aa-92dc-4d17-9594-a9e6192155b1",
                    "cast_to_table": null
                  }
                }
              },
              "query_text": "insert EntityChange{\n    entity_name := <str>$entity_name,\n    entity_id := <uuid>$entity_id,\n    execution := <uuid>$execution_id,\n    new := <json>$new\n};"
            }
          }
        ],
        "started_at": "2025-05-11T07:24:29.010114Z",
        "queries_count": 3
      }
    ],
    "standalone_queries": [],
    "transactions_count": 1,
    "standalone_queries_count": 0
  }
}"#;
