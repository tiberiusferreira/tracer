use crate::TraceName;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertConfig {
    pub service_wide: ServiceWideAlertConfig,
    pub trace_wide: TraceWideAlertConfig,
    pub service_alert_config_trace_overwrite: HashMap<TraceName, TraceWideAlertOverwriteConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceWideAlertConfig {
    pub min_instance_count: u64,
    pub max_active_traces_count: u64,
    pub max_export_buffer_usage_percentage: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TraceWideAlertConfig {
    pub max_trace_duration_ms: u64,
    pub max_traces_with_warning_percentage: u64,
    pub percentage_check_time_window_secs: u64,
    pub percentage_check_min_number_samples: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TraceWideAlertOverwriteConfig {
    pub max_trace_duration_ms: u64,
    pub max_traces_with_warning_percentage: u64,
}
