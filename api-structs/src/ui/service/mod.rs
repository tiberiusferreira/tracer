use crate::Endpoint;
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
pub struct NewFiltersRequest {
    pub service_id: i32,
    pub log_filter: String,
}
