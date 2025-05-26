use crate::Endpoint;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub struct GetService;

impl Endpoint for GetService {
    const PATH: &'static str = "/api/ui/service/";
    const METHOD: &'static str = "GET";
    type RequestBody = ();
    type QueryParameters = ();
    type ResponseBody = Vec<Service>;
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Service {
    pub id: i32,
    /// tracer-backend
    pub name: String,
    /// Local
    pub env: crate::Env,
    // info
    pub log_filter: String,
    pub instances: Vec<Instance>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Instance {
    pub id: i32,
    pub log_filter: Option<String>,
    pub registered_at: chrono::DateTime<chrono::Utc>,
    pub last_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub export_buffer_size_bytes: Option<u64>,
    pub has_profile_data: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileData {
    pub profile_data_timestamp: u64,
    pub profile_data: Vec<u8>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Filters {
    pub end_date: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionListFilters {
    pub bucket: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecutionHeader {
    pub id: uuid::Uuid,
    pub service_name: String,
    pub started_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub size_bytes: u64,
    pub status_code: Option<String>,
    pub path: Option<String>,
    pub method: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Summaries {
    pub buckets: Vec<DateTime<Utc>>,
    pub execution: ExecutionSummary,
    pub requests: RequestsSummary,
    pub size_bytes: SizeBytesSummary,
    pub duration: DurationSummary,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecutionSummary {
    pub total: u64,
    pub with_warning_count: u64,
    pub with_errors_count: u64,
    pub values: Vec<f64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RequestsSummary {
    pub total: u64,
    pub with_200_status_count: u64,
    pub with_non_200_status_count: u64,
    pub values: Vec<f64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SizeBytesSummary {
    pub total: u64,
    pub values: Vec<f64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DurationSummary {
    pub max_ms: f64,
    pub max_values: Vec<f64>,
}
