pub use crate::ui::orphan_events::OrphanEvent;
use crate::{ServiceId, TraceName};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

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
    pub instance_id: Uuid,
    pub traces_state: Vec<TraceHeader>,
    pub orphan_events: Vec<OrphanEvent>,
    pub traces_budget_usage: HashMap<TraceName, u32>,
    pub orphan_events_budget_usage: u32,
}

impl ServiceDataOverTime {
    pub fn finished_traces(&self) -> impl Iterator<Item = &TraceHeader> {
        self.traces_state.iter().filter(|t| t.is_closed)
    }
    pub fn active_traces(&self) -> impl Iterator<Item = &TraceHeader> {
        self.traces_state.iter().filter(|t| !t.is_closed)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Instance {
    pub id: Uuid,
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
pub struct TraceHeader {
    pub trace_id: u32,
    pub trace_name: String,
    pub trace_timestamp: u64,
    pub new_warnings: bool,
    pub new_errors: bool,
    pub fragment_bytes: u64,
    pub is_closed: bool,
    pub duration: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewFiltersRequest {
    pub service_id: ServiceId,
    pub instance_id: uuid::Uuid,
    pub filters: String,
}
