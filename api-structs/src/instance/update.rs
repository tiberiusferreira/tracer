use crate::InstanceId;
pub use crate::Severity;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedServiceTraceData {
    pub instance_id: InstanceId,
    pub closed_spans: Vec<ClosedSpan>,
    pub orphan_events: Vec<NewOrphanEvent>,
    pub rust_log: String,
    pub active_trace_fragments: HashMap<u64, TraceFragment>,
    pub producer_stats: ExportBufferStats,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SamplingData {
    pub new_traces_sampling_rate_0_to_1: f32,
    pub existing_traces_new_data_sampling_rate_0_to_1: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Sampling {
    pub traces: HashMap<TraceName, SamplingData>,
    pub orphan_events_sampling_rate_0_to_1: f32,
}

impl Sampling {
    pub fn new_allow_everything() -> Self {
        Self {
            traces: HashMap::new(),
            orphan_events_sampling_rate_0_to_1: 1.0,
        }
    }
}

impl TraceFragment {
    pub fn is_closed(&self, closed_spans: &[ClosedSpan]) -> bool {
        self.duration_if_closed(closed_spans).is_some()
    }
    pub fn has_warnings(&self) -> bool {
        self.new_events
            .iter()
            .any(|event| event.level == Severity::Warn)
    }
    pub fn has_errors(&self) -> bool {
        self.new_events
            .iter()
            .any(|event| event.level == Severity::Error)
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
    pub timestamp: u64,
    pub severity: Severity,
    pub message: Option<String>,
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
pub struct ExportBufferStats {
    pub export_buffer_capacity: u64,
    pub export_buffer_usage: u64,
}

impl ExportBufferStats {
    pub fn usage_percentage_0_to_100(&self) -> f64 {
        (self.export_buffer_usage as f64 / self.export_buffer_capacity as f64) * 100.
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SingleTraceStatus {
    pub spe_usage_per_minute: u64,
    pub traces_dropped_by_sampling_per_minute: u64,
}
