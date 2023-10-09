use std::collections::HashMap;
use std::num::NonZeroU64;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Severity {
    fn as_lowercase_str(&self) -> &'static str {
        match self {
            Severity::Trace => "trace",
            Severity::Debug => "debug",
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Error => "error",
        }
    }
}

impl FromStr for Severity {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "TRACE" => Ok(Self::Trace),
            "DEBUG" => Ok(Self::Debug),
            "INFO" => Ok(Self::Info),
            "WARN" => Ok(Self::Warn),
            "ERROR" => Ok(Self::Error),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SamplerLimits {
    pub span_plus_event_per_minute_per_trace_limit: u32,
    pub logs_per_minute_limit: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub uuid: String,
    pub service_name: String,
    pub env: String,
    pub filters: TracerFilters,
    pub sampler_limits: SamplerLimits,
}

pub type ServiceName = String;
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveInstances {
    pub instances: HashMap<ServiceName, Vec<LiveServiceInstance>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceLogRequest {
    pub service_name: String,
    pub start_time: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Log {
    pub timestamp: u64,
    pub severity: Severity,
    pub value: String,
}

pub type ServiceNameList = Vec<String>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveServiceInstance {
    pub last_seen_timestamp: u64,
    pub service_id: i64,
    pub service_name: String,
    pub filters: String,
    pub tracer_stats: TracerStats,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewFiltersRequest {
    pub instance_id: i64,
    pub filters: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TracerStats {
    pub reconnects: u32,
    pub spe_dropped_on_export: u32,
    pub orphan_events_per_minute_usage: u32,
    pub logs_per_minute_dropped: u32,
    pub per_minute_trace_stats: HashMap<String, TraceApplicationStats>,
    pub sampler_limits: SamplerLimits,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceApplicationStats {
    pub spe_usage_per_minute: u32,
    pub dropped_traces_per_minute: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedServiceTraceData {
    pub service_id: i64,
    pub service_name: String,
    pub events: Vec<SubscriberEvent>,
    pub filters: String,
    pub tracer_stats: TracerStats,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SubscriberEvent {
    NewSpan(NewSpan),
    NewSpanEvent(NewSpanEvent),
    ClosedSpan(ClosedSpan),
    NewOrphanEvent(NewOrphanEvent),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewSpan {
    pub id: NonZeroU64,
    pub trace_id: NonZeroU64,
    pub timestamp: u64,
    pub parent_id: Option<NonZeroU64>,
    pub name: String,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewSpanEvent {
    pub trace_id: NonZeroU64,
    pub span_id: NonZeroU64,
    pub name: String,
    pub timestamp: u64,
    pub level: Severity,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewOrphanEvent {
    pub name: String,
    pub timestamp: u64,
    pub level: Severity,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClosedSpan {
    pub id: NonZeroU64,
    pub duration: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TracerFilters {
    pub global: Severity,
    pub per_crate: HashMap<String, Severity>,
    pub per_span: HashMap<String, Severity>,
}

impl TracerFilters {
    pub fn to_filter_str(&self) -> String {
        let per_crate = self
            .per_crate
            .iter()
            .map(|(crate_name, filter)| format!("{crate_name}={}", filter.as_lowercase_str()))
            .collect::<Vec<String>>()
            .join(",");
        let per_span = self
            .per_span
            .iter()
            .map(|(span_name, filter)| format!("[{span_name}]={}", filter.as_lowercase_str()))
            .collect::<Vec<String>>()
            .join(",");

        let non_empty: Vec<String> = vec![
            self.global.as_lowercase_str().to_string(),
            per_crate,
            per_span,
        ]
        .into_iter()
        .filter(|e| !e.is_empty())
        .collect();
        let filters = non_empty.join(",");
        filters
    }
}
