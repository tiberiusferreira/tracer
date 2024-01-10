use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceGridResponse {
    pub rows: Vec<TraceGridRow>,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceGridRow {
    pub instance_id: i64,
    pub id: i64,
    pub service_name: String,
    pub started_at: u64,
    pub top_level_span_name: String,
    pub duration_ns: Option<u64>,
    pub original_span_count: u64,
    pub original_event_count: u64,
    pub stored_span_count: u64,
    pub stored_event_count: u64,
    pub estimated_size_bytes: u64,
    pub warning_count: u32,
    pub has_errors: bool,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Autocomplete {
    pub service_names: Vec<String>,
    pub top_level_spans: Vec<String>,
    // pub spans: Vec<String>,
    // pub keys: Vec<String>,
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
