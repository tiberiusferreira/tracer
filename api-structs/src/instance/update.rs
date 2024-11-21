pub use crate::Severity;
use crate::{InstanceId, TraceName};
use std::collections::HashMap;

pub const ROOT_SPAN_ID: u32 = 0;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedServiceTraceData {
    pub instance_id: InstanceId,
    pub orphan_events: Vec<OrphanEvent>,
    pub traces_state: HashMap<u32, TraceState>,
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

/// The first span, with id=0 will always be the root
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceState {
    pub id: u32,
    pub spans: HashMap<u32, Span>,
    pub new_events: Vec<SpanEvent>,
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
    pub fn is_closed(&self) -> bool {
        self.root().closed
    }
    pub fn has_warnings(&self) -> bool {
        self.new_events
            .iter()
            .any(|event| event.severity == Severity::Warn)
    }
    pub fn has_errors(&self) -> bool {
        self.new_events
            .iter()
            .any(|event| event.severity == Severity::Error)
    }

    pub fn root(&self) -> &Span {
        self.spans
            .get(&ROOT_SPAN_ID)
            .expect("trace_state should always have root")
    }
    pub fn root_mut(&mut self) -> &mut Span {
        self.spans
            .get_mut(&ROOT_SPAN_ID)
            .expect("trace_state should always have root")
    }
    pub fn total_size(&self) -> usize {
        let mut total_size = 0;
        for data in self.spans.values() {
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

/// Uniquely identifies a Span globally
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GlobalSpanId {
    instance_id: uuid::Uuid,
    trace_id: u32,
    span_id: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub id: u32,
    pub name: String,
    pub timestamp: u64,
    pub duration: u64,
    pub parent_id: Option<u32>,
    pub key_vals: HashMap<String, String>,
    pub location: Location,
    pub links_to: Option<GlobalSpanId>,
    pub closed: bool,
}

impl Span {
    pub fn refresh_duration(&mut self, now_nanos: u64) {
        if !self.closed {
            let new_duration = now_nanos
                .checked_sub(self.timestamp)
                .expect("duration to never be negative");
            assert!(new_duration >= self.duration, "duration should only go up");
            self.duration = new_duration;
        }
    }
    pub fn close_refreshing_duration(&mut self, now_nanos: u64) {
        self.refresh_duration(now_nanos);
        self.closed = true;
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanEvent {
    pub span_id: u32,
    pub message: Option<String>,
    pub timestamp: u64,
    pub severity: Severity,
    pub key_vals: HashMap<String, String>,
    pub location: Location,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrphanEvent {
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
