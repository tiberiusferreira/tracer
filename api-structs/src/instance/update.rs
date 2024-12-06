pub use crate::Severity;
use deepsize::{Context, DeepSizeOf};
use std::collections::HashMap;

pub struct InstanceUpdateEndpoint;
impl crate::Endpoint for InstanceUpdateEndpoint {
    const PATH: &'static str = "/api/instance/update";
    const METHOD: &'static str = "POST";
    type RequestBody = InstanceSnapshot;
    type ResponseBody = ConfigChange;
}

pub const ROOT_SPAN_ID: u64 = 0;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfigChange {
    pub log_filter: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstanceSnapshot {
    /// This id should be incremented sequentially after a given update is successfully sent
    /// The client is required to retry sending the update until it receives an OK response back
    /// but it could accidentally send it twice, due to a timeout that would eventually be an OK
    /// This id helps the Collector discard duplicate updates
    pub id: u64,
    pub instance_id: uuid::Uuid,
    pub trace_snapshots: HashMap<crate::TraceId, TraceSnapshot>,
    pub orphan_events: Vec<Event>,
    pub export_buffer_size_bytes: u64,
    pub log_filter: String,
    pub cpu_profile_base64: Option<String>,
}

/// The first span, with id=0 will always be the root
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, DeepSizeOf)]
pub struct TraceSnapshot {
    pub trace_id: crate::TraceId,
    pub spans: HashMap<crate::SpanId, Span>,
}

impl TraceSnapshot {
    pub fn is_closed(&self) -> bool {
        self.root().is_closed
    }
    pub fn warning_count(&self) -> u32 {
        self.spans
            .values()
            .filter(|e| e.events.iter().any(|e| e.severity == Severity::Warn))
            .count() as u32
    }
    pub fn has_errors(&self) -> bool {
        self.spans
            .values()
            .any(|e| e.events.iter().any(|e| e.severity == Severity::Error))
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub id: crate::SpanId,
    pub name: String,
    pub created_at_timestamp: u64,
    pub duration: u64,
    pub events: Vec<Event>,
    pub parent_id: Option<crate::SpanId>,
    pub attributes: HashMap<String, serde_json::Value>,
    pub location: Location,
    pub links_to: Option<GlobalSpanId>,
    pub is_closed: bool,
}

fn size_of_attributes(
    attributes: &HashMap<String, serde_json::Value>,
    context: &mut Context,
) -> usize {
    // todo: fix
    format!("{:?}", attributes).deep_size_of_children(context)
}
impl DeepSizeOf for Span {
    fn deep_size_of_children(&self, context: &mut Context) -> usize {
        self.id.deep_size_of_children(context)
            + self.name.deep_size_of_children(context)
            + self.created_at_timestamp.deep_size_of_children(context)
            + self.duration.deep_size_of_children(context)
            + self.events.deep_size_of_children(context)
            + self.parent_id.deep_size_of_children(context)
            + size_of_attributes(&self.attributes, context)
            + self.location.deep_size_of_children(context)
            + self.links_to.deep_size_of_children(context)
            + self.is_closed.deep_size_of_children(context)
    }
}

impl Span {
    pub fn refresh_duration(&mut self, now_nanos: u64) {
        if !self.is_closed {
            let new_duration = now_nanos
                .checked_sub(self.created_at_timestamp)
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Event {
    pub message: Option<String>,
    pub timestamp: u64,
    pub severity: Severity,
    pub attributes: HashMap<String, serde_json::Value>,
    pub location: Location,
}

impl DeepSizeOf for Event {
    fn deep_size_of_children(&self, context: &mut Context) -> usize {
        self.message.deep_size_of_children(context)
            + self.timestamp.deep_size_of_children(context)
            + self.severity.deep_size_of_children(context)
            + size_of_attributes(&self.attributes, context)
            + self.location.deep_size_of_children(context)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, DeepSizeOf)]
pub struct Location {
    pub module: Option<String>,
    pub filename: Option<String>,
    pub line: Option<u32>,
}
