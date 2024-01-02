use crate::Env;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceHealth {
    /// tracer-backend
    pub name: String,
    /// Local
    pub env: Env,
    pub alert_config: AlertConfig,
    pub instances: Vec<Instance>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Instance {
    pub id: i64,
    /// info
    pub rust_log: String,
    // time data
    pub time_data_points: Vec<InstanceDataPoint>,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertConfig {
    graph_alert_config: ServiceAlertConfig,
    trace_alert_config: TraceAlertConfig,
    trace_alert_overwrite_config: TraceAlertOverwriteConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceAlertConfig {
    pub min_instance_count: u32,
    pub max_active_trace: u32,
    pub traces_dropped_per_min: u32,
    pub spe_per_min: u32,
    pub log_per_min: u32,
    pub log_dropped_per_min: u32,
    pub events_kb_per_min: u32,
    pub export_buffer_usage: u32,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TraceAlertOverwriteConfig {
    pub trace_to_overwrite_config: HashMap<String, TraceAlertConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TraceAlertConfig {
    max_traces_with_warning_percentage: u8,
    max_traces_with_error_percentage: u8,
    max_trace_duration: u8,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InstanceDataPoint {
    pub timestamp: u64,
    pub active_traces: Vec<ActiveTrace>,
    pub spe_per_min: u64,
    pub log_per_min: u64,
    pub logs_dropped_per_min: u64,
    pub traces_dropped_per_min: u64,
    pub export_buffer_capacity: u64,
    pub export_buffer_usage: u64,
    pub traces_kb_per_min: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActiveTrace {
    pub trace_id: u64,
    pub trace_name: String,
    pub trace_timestamp: u64,
    pub spe_usage_per_minute: u64,
    pub traces_dropped_by_sampling_per_minute: u64,
}
