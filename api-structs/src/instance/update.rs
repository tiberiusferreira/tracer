use crate::InstanceId;
pub use crate::Severity;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedServiceTraceData {
    pub instance_id: InstanceId,
    pub closed_spans: Vec<ClosedSpan>,
    pub orphan_events: Vec<NewOrphanEvent>,
    pub rust_log: String,
    pub active_trace_fragments: HashMap<u64, TraceFragment>,
    pub producer_stats: ProducerStats,
    pub profile_data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceFragment {
    pub trace_id: u64,
    pub trace_name: String,
    pub trace_timestamp: u64,
    pub spe_count: SpanEventCount,
    pub new_spans: Vec<NewSpan>,
    pub new_events: Vec<NewSpanEvent>,
}

impl TraceFragment {
    pub fn is_closed(&self, closed_spans: &[ClosedSpan]) -> bool {
        self.duration_if_closed(closed_spans).is_some()
    }
    pub fn duration_if_closed(&self, closed_spans: &[ClosedSpan]) -> Option<u64> {
        let root_closed = self.new_spans.iter().find_map(|span| {
            if let Some(duration) = span.duration {
                if span.id == self.trace_id {
                    return Some(duration);
                }
            }
            None
        });
        if let Some(root_duration) = root_closed {
            return Some(root_duration);
        }
        let trace_old_root_duration = closed_spans.iter().find_map(|closed| {
            if closed.trace_id == self.trace_id && closed.span_id == self.trace_id {
                Some(closed.duration)
            } else {
                None
            }
        });
        trace_old_root_duration
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanEventCount {
    pub span_count: u32,
    pub event_count: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewSpan {
    pub id: u64,
    pub timestamp: u64,
    pub duration: Option<u64>,
    pub parent_id: Option<u64>,
    pub name: String,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewSpanEvent {
    pub span_id: u64,
    pub message: Option<String>,
    pub timestamp: u64,
    pub level: Severity,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewOrphanEvent {
    pub message: Option<String>,
    pub timestamp: u64,
    pub level: Severity,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClosedSpan {
    pub trace_id: u64,
    pub span_id: u64,
    pub duration: u64,
}

///////

use crate::TraceName;
use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProducerStats {
    // 1 graph
    pub spe_buffer_capacity: u64,
    pub spe_buffer_usage: u64,
    //
    // 2 graph
    pub orphan_events_per_minute_usage: u64,
    pub orphan_events_dropped_by_sampling_per_minute: u64,
    //
    // 3 graph
    pub spe_dropped_due_to_full_export_buffer_per_min: u64,
    //
    // 4 graph
    // spe_usage_per_minute
    //
    // 5 graph
    // traces dropped per minute
    pub per_minute_trace_stats: HashMap<TraceName, SingleTraceStatus>,
    pub sampler_limits: SamplerLimits,
    //
    // 6 graph -> Traces Received <- allows clicking
    //
    // 7 graph -> Active Traces <- allows clicking
    //
    // 8 graph -> Received Trace kb Est
    //
    // 9 graph -> Received Log kbs Est
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SingleTraceStatus {
    pub spe_usage_per_minute: u64,
    pub traces_dropped_by_sampling_per_minute: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SamplerLimits {
    /// Once this usage limit is reached, new traces will be dropped until the minute is elapsed.
    /// After a minute elapses, the usage is also decreased by this value.
    /// Notice that the usage might go higher than this value, up to
    /// (trace_spe_per_minute_per_trace_limit+extra_spe_per_minute_limit_for_existing_traces)
    pub trace_spe_per_minute_per_trace_limit: u64,
    /// Even if the limit above is hit, existing trace continue recording data until this extra limit is reached
    /// at which point they stop recording data too. This is meant to allow existing traces to complete.
    /// It's usually better to have few complete traces than multiple incomplete ones
    /// This also is the limit for long running traces, for background tasks for example
    pub extra_spe_per_minute_limit_for_existing_traces: u64,
    pub logs_per_minute_limit: u64,
}
