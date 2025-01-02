pub use crate::Severity;
use deepsize::{Context, DeepSizeOf};
use std::collections::HashMap;

pub struct InstanceUpdateEndpoint;
impl crate::Endpoint for InstanceUpdateEndpoint {
    const PATH: &'static str = "/api/instance/update";
    const METHOD: &'static str = "POST";
    type RequestBody = InstanceSnapshot;
    type QueryParameters = ();
    type ResponseBody = ConfigChange;
}

pub const ROOT_SPAN_COUNT_ID: u64 = 1;

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
    pub update_count: u64,
    pub instance_id: uuid::Uuid,
    pub trace_snapshots: HashMap<crate::TraceCountId, TraceFragment>,
    pub orphan_events: Vec<Event>,
    pub export_buffer_size_bytes: u64,
    pub log_filter: String,
    pub cpu_profile_base64: Option<String>,
}

/// The first span, with id=0 will always be the root
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, DeepSizeOf)]
pub struct TraceFragment {
    pub trace_count_id: crate::TraceCountId,
    pub spans: HashMap<crate::SpanCountId, Span>,
}

impl TraceFragment {
    pub fn is_closed(&self) -> bool {
        self.root().has_ended
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
            .get(&ROOT_SPAN_COUNT_ID)
            .expect("trace_state should always have root")
    }
    pub fn root_mut(&mut self) -> &mut Span {
        self.spans
            .get_mut(&ROOT_SPAN_COUNT_ID)
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
    pub id: crate::SpanCountId,
    pub name: String,
    pub started_at_nanos: u64,
    pub duration_nanos: u64,
    pub events: Vec<Event>,
    pub parent_id: Option<crate::SpanCountId>,
    pub attributes: HashMap<String, serde_json::Value>,
    pub location: Location,
    pub links_to: Option<GlobalSpanId>,
    pub has_ended: bool,
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
            + self.started_at_nanos.deep_size_of_children(context)
            + self.duration_nanos.deep_size_of_children(context)
            + self.events.deep_size_of_children(context)
            + self.parent_id.deep_size_of_children(context)
            + size_of_attributes(&self.attributes, context)
            + self.location.deep_size_of_children(context)
            + self.links_to.deep_size_of_children(context)
            + self.has_ended.deep_size_of_children(context)
    }
}

impl Span {
    pub fn refresh_duration(&mut self, now_nanos: u64) {
        if !self.has_ended {
            let new_duration = now_nanos
                .checked_sub(self.started_at_nanos)
                .expect("duration to never be negative");
            assert!(new_duration >= self.duration_nanos, "duration should only go up");
            self.duration_nanos = new_duration;
        }
    }
    pub fn close_refreshing_duration(&mut self, now_nanos: u64) {
        self.refresh_duration(now_nanos);
        self.has_ended = true;
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
