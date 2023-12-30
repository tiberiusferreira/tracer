use crate::TraceName;
use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TracerStatus {
    pub spe_buffer_capacity: u32,
    pub sampler_limits: SamplerLimits,
    pub spe_buffer_usage: u32,
    pub spe_dropped_due_to_full_export_buffer: u32,
    pub orphan_events_per_minute_usage: u32,
    pub orphan_events_dropped_by_sampling_per_minute: u32,
    pub per_minute_trace_stats: HashMap<TraceName, SingleTraceStatus>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SingleTraceStatus {
    pub spe_usage_per_minute: u32,
    pub traces_dropped_by_sampling_per_minute: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SamplerLimits {
    /// Once this usage limit is reached, new traces will be dropped until the minute is elapsed.
    /// After a minute elapses, the usage is also decreased by this value.
    /// Notice that the usage might go higher than this value, up to
    /// (trace_spe_per_minute_per_trace_limit+extra_spe_per_minute_limit_for_existing_traces)
    pub trace_spe_per_minute_per_trace_limit: u32,
    /// Even if the limit above is hit, existing trace continue recording data until this extra limit is reached
    /// at which point they stop recording data too. This is meant to allow existing traces to complete.
    /// It's usually better to have few complete traces than multiple incomplete ones
    /// This also is the limit for long running traces, for background tasks for example
    pub extra_spe_per_minute_limit_for_existing_traces: u32,
    pub logs_per_minute_limit: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TracerStatus2 {
    pub sampler_limits: Limits,
    pub spe_dropped_due_to_full_export_buffer: u32,
    pub orphan_events_per_minute_usage: u32,
    pub orphan_events_dropped_by_sampling_per_minute: u32,
    pub per_minute_trace_stats: HashMap<TraceName, SingleTraceStatus>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Limits {
    pub spe_buffer_capacity: u32,
    /// Once this usage limit is reached, new traces will be dropped until the minute is elapsed.
    /// After a minute elapses, the usage is also decreased by this value.
    /// Notice that the usage might go higher than this value, up to
    /// (trace_spe_per_minute_per_trace_limit+extra_spe_per_minute_limit_for_existing_traces)
    pub trace_spe_per_minute_per_trace_limit: u32,
    /// Even if the limit above is hit, existing trace continue recording data until this extra limit is reached
    /// at which point they stop recording data too. This is meant to allow existing traces to complete.
    /// It's usually better to have few complete traces than multiple incomplete ones
    /// This also is the limit for long running traces, for background tasks for example
    pub extra_spe_per_minute_limit_for_existing_traces: u32,
    pub logs_per_minute_limit: u32,
}
