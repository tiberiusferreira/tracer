use serde::{Deserialize, Serialize};
pub mod exporter;
pub mod sse;
pub mod time_conversion;
use serde_with::serde_as;
use serde_with::DisplayFromStr;

pub const FRONTEND_PUBLIC_URL_PATH_NO_TRAILING_SLASH: &str =
    env!("FRONTEND_PUBLIC_URL_PATH_NO_TRAILING_SLASH");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTraceGridRow {
    pub service_id: i64,
    pub id: i64,
    pub service_name: String,
    pub timestamp: u64,
    pub top_level_span_name: String,
    pub duration_ns: Option<u64>,
    pub warning_count: u32,
    pub has_errors: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Autocomplete {
    pub service_names: Vec<String>,
    pub top_level_spans: Vec<String>,
    // pub spans: Vec<String>,
    // pub keys: Vec<String>,
}

#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct TraceId {
    #[serde_as(as = "DisplayFromStr")]
    pub service_id: i64,
    #[serde_as(as = "DisplayFromStr")]
    pub trace_id: i64,
}

#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct TraceChunkId {
    #[serde_as(as = "DisplayFromStr")]
    pub start_timestamp: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub end_timestamp: u64,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct SingleChunkTraceQuery {
    #[serde(flatten)]
    pub trace_id: TraceId,
    #[serde(flatten)]
    pub chunk_id: TraceChunkId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub id: i64,
    pub timestamp: u64,
    pub parent_id: Option<i64>,
    pub duration: Option<u64>,
    pub name: String,
    pub events: Vec<Events>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub user_generated: bool,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    #[serde(alias = "trace")]
    Trace,
    #[serde(alias = "debug")]
    Debug,
    #[serde(alias = "info")]
    Info,
    #[serde(alias = "warn")]
    Warn,
    #[serde(alias = "error")]
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Events {
    pub timestamp: u64,
    pub name: String,
    pub severity: Severity,
    // pub key_values: Vec<KeyValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub service_name: String,
    pub top_level_span_name: String,
    pub total_traces: i64,
    pub total_traces_with_error: i64,
    pub longest_trace_id: u64,
    pub longest_trace_duration: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryRequest {
    pub from_date_unix_micros: u64,
    pub to_date_unix_micros: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchFor {
    pub from_date_unix: u64,
    pub to_date_unix: u64,
    pub service_name: String,
    pub top_level_span: String,
    // pub span: String,
    pub min_duration: u64,
    pub max_duration: Option<u64>,
    pub min_warns: u32,
    // pub key: String,
    // pub value: String,
    // pub event_name: String,
    pub only_errors: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderBy {
    DateDesc,
    DurationAsc,
    DurationDesc,
}
