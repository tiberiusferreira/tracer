use std::collections::HashMap;
use std::str::FromStr;
pub mod exporter;
pub mod time_conversion;
pub mod ui;

pub const FRONTEND_PUBLIC_URL_PATH_NO_TRAILING_SLASH: &str =
    env!("FRONTEND_PUBLIC_URL_PATH_NO_TRAILING_SLASH");

pub type TraceName = String;
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TracerStats {
    pub spe_dropped_on_export: u32,
    pub orphan_events_per_minute_usage: u32,
    pub logs_per_minute_dropped: u32,
    pub per_minute_trace_stats: HashMap<TraceName, SingleTraceStat>,
    pub sampler_limits: SamplerLimits,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SingleTraceStat {
    pub spe_usage_per_minute: u32,
    pub dropped_traces_per_minute: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SamplerLimits {
    /// After this limit is reached, new traces will be dropped until the minute is elapsed
    pub new_trace_span_plus_event_per_minute_per_trace_limit: u32,
    /// Even if the limit above is hit, existing trace continue recording data until this limit is reached
    /// at which point they stop recording data too. This is meant to allow existing traces to complete.
    /// It's usually better to have few complete traces than multiple incomplete ones
    /// This also is the limit for long running traces, for background tasks for example
    pub existing_trace_span_plus_event_per_minute_limit: u32,
    pub logs_per_minute_limit: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
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
