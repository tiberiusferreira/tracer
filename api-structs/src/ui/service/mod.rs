use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use indexmap::IndexMap;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileData {
    pub profile_data_timestamp: u64,
    pub profile_data: Vec<u8>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SummaryFilters {
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub attributes: IndexMap<String, Option<String>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionListFilters {
    pub bucket: DateTime<Utc>,
    pub attributes: IndexMap<String, Option<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecutionHeader {
    pub external_id: uuid::Uuid,
    pub service_name: String,
    pub started_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub size_bytes: u64,
    pub status_code: Option<String>,
    pub path: Option<String>,
    pub method: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AttributeSummary {
    pub name: String,
    pub count: u32,
    pub values: HashMap<String, u32>,
}

type EnvName = String;
type ServiceName = String;
type InstanceId = Uuid;
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SummariesForGraph {
    pub buckets: Vec<DateTime<Utc>>,
    pub execution: ExecutionSummary,
    pub requests: RequestsSummary,
    pub size_bytes: SizeBytesSummary,
    pub duration: DurationSummary,
    pub attributes: HashMap<String, AttributeSummary>,
    pub envs: HashMap<EnvName, EnvSummary>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvSummary {
    pub name: String,
    pub execution_count: u32,
    pub services: HashMap<ServiceName, ServiceSummary>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServiceSummary {
    pub name: String,
    pub execution_count: u32,
    pub instances: HashMap<InstanceId, InstanceSummary>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InstanceSummary {
    pub instance_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub has_cpu_profile: bool,
    pub execution_count: u32,
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
