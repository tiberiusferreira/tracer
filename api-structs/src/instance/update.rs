use crate::InstanceId;
pub use crate::Severity;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedServiceTraceData {
    pub instance_id: InstanceId,
    pub orphan_events: Vec<NewOrphanEvent>,
    pub traces_state: HashMap<u64, TraceState>,
    pub rust_log: String,
    pub profile_data: Option<Vec<u8>>,
}

impl ExportedServiceTraceData {
    pub fn orphan_events_size(&self) -> usize {
        let mut received_orphan_event_bytes = 0;
        for e in &self.orphan_events {
            received_orphan_event_bytes += e.message.as_ref().map(|m| m.len()).unwrap_or(0);
            received_orphan_event_bytes += key_val_size(&e.key_vals);
            received_orphan_event_bytes += e.location.size_bytes();
        }
        received_orphan_event_bytes
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceState {
    pub root_span: RootSpan,
    pub open_spans: HashMap<u64, OpenSpan>,
    pub spans_produced: u32,
    pub events_produced: u32,
    pub events_dropped_by_sampling: u32,
    pub closed_spans: Vec<ClosedSpan>,
    pub new_events: Vec<NewSpanEvent>,
}

fn key_val_size(kv: &HashMap<String, String>) -> usize {
    let mut total = 0;
    for (k, v) in kv {
        total += k.len();
        total += v.len();
    }
    total
}
impl TraceState {
    pub fn total_size(&self) -> usize {
        let mut total_size = 0;
        total_size += self.root_span.name.len();
        total_size += self.root_span.location.size_bytes();
        total_size += key_val_size(&self.root_span.key_vals);
        for data in self.open_spans.values() {
            total_size += data.name.len();
            total_size += key_val_size(&data.key_vals);
            total_size += data.location.size_bytes();
        }
        for data in &self.closed_spans {
            total_size += data.name.len();
            total_size += key_val_size(&data.key_vals);
            total_size += data.location.size_bytes();
        }
        for data in &self.new_events {
            total_size += data.message.as_ref().map(|m| m.len()).unwrap_or(0);
            total_size += key_val_size(&data.key_vals);
            total_size += data.location.size_bytes();
        }
        total_size
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SamplingState {
    AllowNewTraces,
    DropNewTracesKeepExistingTraceNewData,
    DropNewTracesAndNewExistingTracesData,
}

impl SamplingState {
    pub fn allow_new_traces(&self) -> bool {
        matches!(self, SamplingState::AllowNewTraces)
    }
    pub fn allow_existing_trace_new_data(&self) -> bool {
        matches!(self, SamplingState::AllowNewTraces)
            || matches!(self, SamplingState::DropNewTracesKeepExistingTraceNewData)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Sampling {
    pub traces: HashMap<TraceName, SamplingState>,
    pub allow_new_orphan_events: bool,
}

impl Sampling {
    pub fn new_allow_everything() -> Self {
        Self {
            traces: HashMap::new(),
            allow_new_orphan_events: true,
        }
    }
}

impl TraceState {
    pub fn is_closed(&self) -> bool {
        self.root_span.duration.is_some()
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

    pub fn duration_if_closed(&self) -> Option<u64> {
        self.root_span.duration
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RootSpan {
    pub id: u64,
    pub name: String,
    pub timestamp: u64,
    pub duration: Option<u64>,
    pub key_vals: HashMap<String, String>,
    pub location: Location,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpenSpan {
    pub id: u64,
    pub name: String,
    pub timestamp: u64,
    pub parent_id: u64,
    pub key_vals: HashMap<String, String>,
    pub location: Location,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClosedSpan {
    pub id: u64,
    pub name: String,
    pub timestamp: u64,
    pub duration: u64,
    pub parent_id: u64,
    pub key_vals: HashMap<String, String>,
    pub location: Location,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewSpan {
    pub id: u64,
    pub name: String,
    pub timestamp: u64,
    pub duration: Option<u64>,
    pub parent_id: Option<u64>,
    pub key_vals: HashMap<String, String>,
    pub location: Location,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewSpanEvent {
    pub span_id: u64,
    pub message: Option<String>,
    pub timestamp: u64,
    pub level: Severity,
    pub key_vals: HashMap<String, String>,
    pub location: Location,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewOrphanEvent {
    pub timestamp: u64,
    pub severity: Severity,
    pub message: Option<String>,
    pub key_vals: HashMap<String, String>,
    pub location: Location,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Location {
    pub module: Option<String>,
    pub filename: Option<String>,
    pub line: Option<u32>,
}

impl Location {
    fn size_bytes(&self) -> usize {
        let mut size = 0;
        size += self.module.as_ref().map(|e| e.len()).unwrap_or(0);
        size += self.filename.as_ref().map(|e| e.len()).unwrap_or(0);
        size
    }
}

///////

use crate::TraceName;
use std::collections::HashMap;
