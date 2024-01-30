use crate::instance::update::ProducerStats;
use crate::ServiceId;
use serde::{Deserialize, Serialize};
pub mod alerts;
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceOverview {
    pub service_id: ServiceId,
    pub alert_config: alerts::AlertConfig,
    pub instances: Vec<Instance>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Instance {
    pub id: i64,
    pub rust_log: String,
    pub profile_data: Option<ProfileData>,
    pub time_data_points: Vec<InstanceDataPoint>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileData {
    pub profile_data_timestamp: u64,
    pub profile_data: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InstanceDataPoint {
    pub timestamp: u64,
    pub tracer_status: ProducerStats,
    pub active_traces: Vec<TraceHeader>,
    pub finished_traces: Vec<TraceHeader>,
    pub received_spe: u64,
    pub received_trace_bytes: u64,
    pub received_orphan_event_bytes: u64,
}

impl InstanceDataPoint {
    pub fn active_and_finished_iter(&self) -> impl Iterator<Item = &TraceHeader> {
        self.active_traces.iter().chain(self.finished_traces.iter())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TraceHeader {
    pub trace_id: u64,
    pub trace_name: String,
    pub trace_timestamp: u64,
    pub new_warnings: bool,
    pub new_errors: bool,
    pub duration: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewFiltersRequest {
    pub service_id: ServiceId,
    pub instance_id: i64,
    pub filters: String,
}
