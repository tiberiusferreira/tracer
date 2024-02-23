use crate::instance::update::ExportBufferStats;
pub use crate::ui::orphan_events::OrphanEvent;
use crate::{ServiceId, TraceName};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod alerts;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceOverview {
    pub service_id: ServiceId,
    pub alert_config: alerts::AlertConfig,
    pub instances: Vec<Instance>,
    pub service_data_over_time: Vec<ServiceDataOverTime>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceDataOverTime {
    pub timestamp: u64,
    pub instance_id: i64,
    pub export_buffer_stats: ExportBufferStats,
    pub active_traces: Vec<TraceHeader>,
    pub finished_traces: Vec<TraceHeader>,
    pub orphan_events: Vec<OrphanEvent>,
    pub traces_budget_usage: HashMap<TraceName, u32>,
    pub orphan_events_budget_usage: u32,
}

impl ServiceDataOverTime {
    pub fn active_and_finished_iter(&self) -> impl Iterator<Item=&TraceHeader> {
        self.active_traces.iter().chain(self.finished_traces.iter())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Instance {
    pub id: i64,
    pub rust_log: String,
    pub last_seen_secs_ago: u64,
    pub profile_data: Option<ProfileData>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileData {
    pub profile_data_timestamp: u64,
    pub profile_data: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExportBufferOverTime {
    pub timestamp: u64,
    pub tracer_status: ExportBufferStats,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TraceHeader {
    pub trace_id: u64,
    pub trace_name: String,
    pub trace_timestamp: u64,
    pub new_warnings: bool,
    pub new_errors: bool,
    pub fragment_bytes: u64,
    pub duration: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewFiltersRequest {
    pub service_id: ServiceId,
    pub instance_id: i64,
    pub filters: String,
}
