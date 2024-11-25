use std::collections::HashMap;
use tracing::Id;

use crate::subscriber::state::tracer_storage::DataInTracerFormatTrackingStorage;
use api_structs::instance::update::{Event, Location, TraceSnapshot, ROOT_SPAN_ID};
use api_structs::time_conversion::now_nanos_u64;
use api_structs::{Severity, SpanId};

mod tracer_storage;
#[derive(Debug, Clone)]
struct TraceStateWithSpanCount {
    trace_snapshot: TraceSnapshot,
    span_count: u64,
}

#[derive(Debug, Clone)]
pub struct State {
    data: DataInTracerFormatTrackingStorage,
    registry_to_tracer_id_mapping: HashMap<Id, u64>,
}

#[derive(Debug, Clone)]
pub struct TracesAndOrphanEvents {
    pub traces: HashMap<u64, TraceSnapshot>,
    pub orphan_events: Vec<Event>,
    pub export_buffer_size_bytes: u64,
}

impl State {
    pub fn new() -> Self {
        Self {
            data: DataInTracerFormatTrackingStorage::new(),
            registry_to_tracer_id_mapping: HashMap::new(),
        }
    }
    pub fn remove_data_ready_to_export(&mut self) -> TracesAndOrphanEvents {
        let mut traces_and_orphan_events = self.data.extract_data_for_export_pruning_internally();
        for trace in traces_and_orphan_events.traces.values_mut() {
            if !trace.is_closed() {
                for span in trace.spans.values_mut() {
                    if !span.is_closed {
                        span.refresh_duration(now_nanos_u64());
                    }
                }
            }
        }
        traces_and_orphan_events
    }

    pub fn insert_new_trace(
        &mut self,
        id: &Id,
        name: String,
        key_vals: HashMap<String, String>,
        location: Location,
    ) {
        let new_tracer_trace_id = self.data.insert_new_trace(name, key_vals, location);
        self.registry_to_tracer_id_mapping
            .insert(id.clone(), new_tracer_trace_id);
    }
    pub fn close_trace(&mut self, trace_id: Id) {
        let tracer_trace_id = self
            .registry_to_tracer_id_mapping
            .remove(&trace_id)
            .expect("id to exist if used");
        self.data.close_trace(tracer_trace_id);
    }
    fn span_to_tracer_span_id(&self, trace_id: Id, span_id: Id) -> SpanId {
        // in Tracing land, there are only spans, but in Tracer land, there are spans and traces.
        // When inserting a trace in our mapping, we map the Root Span to the TraceId.
        // When we get that Id back, we know it for the Trace and its root span, but there is no mapping from the Id
        // to the Trace Root span
        if trace_id == span_id {
            ROOT_SPAN_ID
        } else {
            *self
                .registry_to_tracer_id_mapping
                .get(&span_id)
                .expect("span id to exist")
        }
    }
    pub fn insert_new_span(
        &mut self,
        trace_id: Id,
        span_id: Id,
        parent_id: Id,
        name: String,
        key_vals: HashMap<String, String>,
        location: Location,
    ) {
        let tracer_trace_id = self
            .registry_to_tracer_id_mapping
            .get(&trace_id)
            .expect("trace id to exist if has new span");
        let tracer_parent_id = self.span_to_tracer_span_id(trace_id, parent_id);
        let tracer_new_span_id =
            self.data
                .insert_new_span(*tracer_trace_id, tracer_parent_id, name, key_vals, location);
        self.registry_to_tracer_id_mapping
            .insert(span_id.clone(), tracer_new_span_id);
    }
    pub fn close_span(&mut self, trace_id: Id, span_id: Id) {
        let tracer_trace_id = *self
            .registry_to_tracer_id_mapping
            .get(&trace_id)
            .expect("trace id to exist if has closing span");
        // no need for self.span_to_tracer_span_id(trace_id, span_id);
        // because we never close the root span, we close the trace instead
        let tracer_span_id = self
            .registry_to_tracer_id_mapping
            .remove(&span_id)
            .expect("span id to exist if closing");
        self.data.close_span(tracer_trace_id, tracer_span_id);
    }

    pub fn insert_span_event(
        &mut self,
        trace_id: Id,
        span_id: Id,
        message: Option<String>,
        severity: Severity,
        key_vals: HashMap<String, String>,
        location: Location,
    ) {
        let tracer_trace_id = *self
            .registry_to_tracer_id_mapping
            .get(&trace_id)
            .expect("trace id to exist if has new span event");
        let tracer_span_id = self.span_to_tracer_span_id(trace_id, span_id);
        self.data.create_and_insert_span_event(
            tracer_trace_id,
            tracer_span_id,
            message,
            severity,
            key_vals,
            location,
        );
    }
    pub fn insert_orphan_event(&mut self, orphan_event: Event) {
        self.data.insert_orphan_event(orphan_event);
    }
}
