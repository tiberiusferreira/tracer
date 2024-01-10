use crate::exporter::status::ProducerStats;
use crate::Env;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct ServiceId {
    /// tracer-backend
    pub name: String,
    /// Local
    pub env: Env,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceData {
    /// tracer-backend
    pub name: String,
    /// Local
    pub env: Env,
    pub alert_config: AlertConfig,
    pub instances: Vec<Instance>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileData {
    pub profile_data_timestamp: u64,
    pub profile_data: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Instance {
    pub id: i64,
    /// info
    pub rust_log: String,
    pub profile_data: Option<ProfileData>,
    // time data
    pub time_data_points: Vec<InstanceDataPoint>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertConfig {
    pub service_alert_config: ServiceAlertConfig,
    pub service_alert_config_trace_overwrite: ServiceAlertConfigTraceOverwrite,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceAlertConfig {
    pub min_instance_count: u64,
    pub max_active_traces: u64,
    pub max_spe_export_buffer_usage: u64,
    pub max_orphan_events_per_min: u64,
    pub max_orphan_events_dropped_by_sampling_per_min: u64,
    pub max_spe_dropped_due_to_full_export_buffer_per_min: u64,
    pub max_received_spe: u64,
    pub max_received_trace_kb: u64,
    pub max_received_orphan_event_kb: u64,
    pub trace_alert_config: TraceAlertConfig,
    pub percentage_check_time_window_secs: u64,
    pub percentage_check_min_number_samples: u64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceAlertConfigTraceOverwrite {
    pub trace_to_overwrite_config: HashMap<String, TraceAlertConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TraceAlertConfig {
    pub max_trace_duration_ms: u64,
    pub max_traces_with_warning_percentage: u64,
    pub max_traces_with_error_percentage: u64,
    pub max_traces_dropped_by_sampling_per_min: u64,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TraceHeader {
    pub trace_id: u64,
    pub trace_name: String,
    pub trace_timestamp: u64,
    pub duration: Option<u64>,
    // pub spe_usage_per_minute: u64,
    // pub traces_dropped_by_sampling_per_minute: u64,
}
