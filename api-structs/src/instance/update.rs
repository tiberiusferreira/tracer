pub use crate::Severity;
use deepsize::{Context, DeepSizeOf};
use std::collections::HashMap;

pub const ROOT_SPAN_ID: u64 = 0;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstanceSnapshot {
    pub instance_id: uuid::Uuid,
    pub orphan_events: Vec<Event>,
    pub trace_snapshots: HashMap<crate::TraceId, TraceSnapshot>,
    pub export_buffer_size_bytes: u64,
    pub log_filter: String,
    pub cpu_profile: Option<Vec<u8>>,
}

/// The first span, with id=0 will always be the root
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, DeepSizeOf)]
pub struct TraceSnapshot {
    pub id: u64,
    pub spans: HashMap<u64, Span>,
}

impl TraceSnapshot {
    pub fn is_closed(&self) -> bool {
        self.root().is_closed
    }
    // pub fn has_warnings(&self) -> bool {
    //     self.new_events
    //         .iter()
    //         .any(|event| event.severity == Severity::Warn)
    // }
    // pub fn has_errors(&self) -> bool {
    //     self.new_events
    //         .iter()
    //         .any(|event| event.severity == Severity::Error)
    // }

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
}
/// Uniquely identifies a Span globally
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GlobalSpanId {
    pub instance_id: uuid::Uuid,
    pub trace_id: u32,
    pub span_id: u32,
}

impl DeepSizeOf for GlobalSpanId {
    fn deep_size_of_children(&self, context: &mut Context) -> usize {
        size_of::<uuid::Uuid>()
            + self.trace_id.deep_size_of_children(context)
            + self.span_id.deep_size_of_children(context)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, DeepSizeOf)]
pub struct Span {
    pub id: u64,
    pub name: String,
    pub timestamp: u64,
    pub duration: u64,
    pub events: Vec<Event>,
    pub parent_id: Option<crate::SpanId>,
    pub key_vals: HashMap<String, String>,
    pub location: Location,
    pub links_to: Option<GlobalSpanId>,
    pub is_closed: bool,
}

impl Span {
    pub fn refresh_duration(&mut self, now_nanos: u64) {
        if !self.is_closed {
            let new_duration = now_nanos
                .checked_sub(self.timestamp)
                .expect("duration to never be negative");
            assert!(new_duration >= self.duration, "duration should only go up");
            self.duration = new_duration;
        }
    }
    pub fn close_refreshing_duration(&mut self, now_nanos: u64) {
        self.refresh_duration(now_nanos);
        self.is_closed = true;
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, DeepSizeOf)]
pub struct Event {
    pub message: Option<String>,
    pub timestamp: u64,
    pub severity: Severity,
    pub key_vals: HashMap<String, String>,
    pub location: Location,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, DeepSizeOf)]
pub struct Location {
    pub module: Option<String>,
    pub filename: Option<String>,
    pub line: Option<u32>,
}
