use crate::exporter::status::ProducerStats;
use crate::Env;
pub use crate::Severity;
use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedServiceTraceData {
    pub service_name: String,
    pub env: Env,
    pub instance_id: i64,
    pub closed_spans: Vec<ClosedSpan>,
    pub orphan_events: Vec<NewOrphanEvent>,
    pub rust_log: String,
    pub active_trace_fragments: HashMap<u64, TraceFragment>,
    pub producer_stats: ProducerStats,
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
        let root_closed = self
            .new_spans
            .iter()
            .any(|span| span.id == self.trace_id && span.duration.is_some());
        if root_closed {
            return true;
        }
        let trace_old_root_closed = closed_spans
            .iter()
            .any(|closed| closed.trace_id == self.trace_id && closed.span_id == self.trace_id);
        if trace_old_root_closed {
            return true;
        }
        false
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
