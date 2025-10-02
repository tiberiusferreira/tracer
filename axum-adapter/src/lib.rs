use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use base64::Engine;
use http::Method;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecordedRequest {
    pub parts: RecordedRequestParts,
    pub body_base64: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum RecordedMethod {
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

impl Display for RecordedMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordedMethod::Options => f.write_str("options"),
            RecordedMethod::Get => f.write_str("get"),
            RecordedMethod::Post => f.write_str("post"),
            RecordedMethod::Put => f.write_str("put"),
            RecordedMethod::Delete => f.write_str("delete"),
            RecordedMethod::Head => f.write_str("head"),
            RecordedMethod::Trace => f.write_str("trace"),
            RecordedMethod::Connect => f.write_str("connect"),
            RecordedMethod::Patch => f.write_str("patch"),
        }
    }
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecordedRequestParts {
    pub method: RecordedMethod,
    pub uri: String,
    pub headers: HashMap<String, String>,
}

pub fn recorded_request_to_axum(request: RecordedRequest) -> axum::extract::Request {
    let builder = http::request::Builder::new();
    let method = match request.parts.method {
        RecordedMethod::Options => &Method::OPTIONS,
        RecordedMethod::Get => &Method::GET,
        RecordedMethod::Post => &Method::POST,
        RecordedMethod::Put => &Method::PUT,
        RecordedMethod::Delete => &Method::DELETE,
        RecordedMethod::Head => &Method::HEAD,
        RecordedMethod::Trace => &Method::TRACE,
        RecordedMethod::Connect => &Method::CONNECT,
        RecordedMethod::Patch => &Method::PATCH,
    };
    let mut builder = builder.uri(request.parts.uri).method(method);
    for (k, v) in &request.parts.headers {
        builder = builder.header(k.to_string(), v.to_string());
    }
    let body_bytes = base64::engine::general_purpose::STANDARD.decode(request.body_base64).expect("Invalid base64 in body");
    let axum_body = axum::body::Body::new(axum::body::Body::from(body_bytes));
    let w = builder.body(axum_body).unwrap();
    w
}
pub async fn axum_request_to_serializable(request: axum::extract::Request) -> RecordedRequest {
    let uri = request.uri().to_string();
    let method = match request.method() {
        &Method::OPTIONS => RecordedMethod::Options,
        &Method::GET => RecordedMethod::Get,
        &Method::POST => RecordedMethod::Post,
        &Method::PUT => RecordedMethod::Put,
        &Method::DELETE => RecordedMethod::Delete,
        &Method::HEAD => RecordedMethod::Head,
        &Method::TRACE => RecordedMethod::Trace,
        &Method::CONNECT => RecordedMethod::Connect,
        &Method::PATCH => RecordedMethod::Patch,
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
    let body_base64 = base64::engine::general_purpose::STANDARD.encode(&body_bytes);
    RecordedRequest {
        parts: RecordedRequestParts {
            method,
            uri,
            headers,
        },
        body_base64,
    }
}
