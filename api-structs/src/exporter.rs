use std::collections::HashMap;
use std::num::NonZeroU64;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Trace {
    pub service_name: String,
    pub id: NonZeroU64,
    pub name: String,
    pub start: u64,
    pub duration: u64,
    pub key_vals: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
    // we keep a list of the placeholder children here so we can remove
    // them from the span_id_to_root_id list later
    pub children: Vec<Span>,
    pub partial: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub id: NonZeroU64,
    pub name: String,
    pub parent_id: NonZeroU64,
    pub start: u64,
    pub duration: u64,
    pub key_vals: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: u64,
    pub level: String,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceSummary {
    pub id: NonZeroU64,
    pub name: String,
    pub duration: u64,
    pub spans: usize,
    pub events: usize,
    pub partial: bool,
}
impl Trace {
    pub fn summary(&self) -> TraceSummary {
        TraceSummary {
            id: self.id,
            name: self.name.clone(),
            duration: self.duration,
            spans: self.spans(),
            events: self.events(),
            partial: self.partial,
        }
    }
    pub fn spans(&self) -> usize {
        // 1 from root
        1 + self.children.len()
    }
    pub fn events(&self) -> usize {
        let self_events = self.events.len();
        let children_events = self
            .children
            .iter()
            .fold(0, |acc, curr| curr.events.len().saturating_add(acc));
        self_events + children_events
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatusData {
    pub service_name: String,
    pub sampler_status: SamplerStatus,
    pub errors: Vec<String>,
    pub active_traces: Vec<TraceSummary>,
    pub export_queue: Vec<TraceSummary>,
    pub orphan_events: Vec<SpanEvent>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SamplerStatus {
    pub hard_se_storage_limit: usize,
    pub hard_limit_hit: bool,
    pub window_duration: u64,
    pub trace_se_quota_per_window: i64,
}
