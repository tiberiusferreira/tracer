use maplit::hashmap;
use std::collections::HashMap;
use tracing::Id;

use api_structs::instance::update::{Location, OrphanEvent, Span, SpanEvent, TraceState};
use api_structs::time_conversion::now_nanos_u64;
use api_structs::Severity;

pub const ROOT_SPAN_ID: u32 = 1;
#[derive(Debug, Clone)]
pub struct State {
    traces: HashMap<u32, TraceState>,
    orphan_events: Vec<OrphanEvent>,
    registry_to_tracer_id_mapping: HashMap<Id, u32>,
    trace_count: u32,
}

#[derive(Debug, Clone)]
pub struct TracesAndOrphanEvents {
    pub traces: HashMap<u32, TraceState>,
    pub orphan_events: Vec<OrphanEvent>,
}

impl State {
    pub fn new() -> Self {
        Self {
            traces: HashMap::new(),
            orphan_events: vec![],
            registry_to_tracer_id_mapping: HashMap::new(),
            trace_count: 0,
        }
    }
    pub fn get_export_data(&mut self) -> TracesAndOrphanEvents {
        let orphan_events = std::mem::take(&mut self.orphan_events);
        let mut traces_snapshot = self.traces.clone();
        // only retain traces still running
        self.traces.retain(|_k, v| !v.is_closed());
        for trace in self.traces.values_mut() {
            trace.spans.retain(|_id, span| !span.closed);
            trace.new_events.clear();
        }
        let now = now_nanos_u64();
        for trace in traces_snapshot.values_mut() {
            for span in trace.spans.values_mut() {
                span.refresh_duration(now);
            }
        }
        TracesAndOrphanEvents {
            traces: traces_snapshot,
            orphan_events,
        }
    }

    pub fn insert_new_trace(
        &mut self,
        id: &Id,
        name: String,
        key_vals: HashMap<String, String>,
        location: Location,
    ) {
        self.trace_count += 1;
        let new_id = self.trace_count;
        self.registry_to_tracer_id_mapping
            .insert(id.clone(), new_id);

        let span = Span {
            id: ROOT_SPAN_ID,
            name,
            timestamp: now_nanos_u64(),
            duration: 0,
            parent_id: None,
            key_vals,
            location,
            closed: false,
        };
        let existing = self.traces.insert(
            new_id,
            TraceState {
                id: new_id,
                root_span_id: span.id,
                spans: hashmap! {span.id => span},
                spans_produced: 1,
                events_produced: 0,
                events_dropped_by_sampling: 0,
                new_events: vec![],
            },
        );
        assert!(existing.is_none());
    }
    pub fn close_trace(&mut self, trace_id: Id) {
        let id = self
            .registry_to_tracer_id_mapping
            .remove(&trace_id)
            .expect("id to exist if used");
        let trace = self
            .traces
            .get_mut(&id)
            .expect("trace to exist when closing");
        let root = trace.root_mut();
        root.close_refreshing_duration(now_nanos_u64());
        if trace.spans.values().any(|s| !s.closed) {
            panic!("trace closed before all spans had closed!");
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
        let mapped_trace_id = self
            .registry_to_tracer_id_mapping
            .get(&trace_id)
            .expect("trace id to exist if has new span");
        let trace = self
            .traces
            .get_mut(mapped_trace_id)
            .expect("trace to exist if it has a new span");
        trace.spans_produced += 1;
        let new_span_id = trace.spans_produced;
        self.registry_to_tracer_id_mapping
            .insert(span_id.clone(), new_span_id);
        let mapped_parent_id = if trace_id == parent_id {
            ROOT_SPAN_ID
        } else {
            *self
                .registry_to_tracer_id_mapping
                .get(&parent_id)
                .expect("parent id to exist")
        };

        let existing = trace.spans.insert(
            new_span_id,
            Span {
                id: new_span_id,
                name,
                timestamp: now_nanos_u64(),
                duration: 0,
                parent_id: Some(mapped_parent_id),
                key_vals,
                location,
                closed: false,
            },
        );
        assert!(existing.is_none());
    }
    pub fn close_span(&mut self, trace_id: Id, span_id: Id) {
        let mapped_trace_id = self
            .registry_to_tracer_id_mapping
            .get(&trace_id)
            .expect("trace id to exist");
        let trace = self
            .traces
            .get_mut(mapped_trace_id)
            .expect("trace to exist if it has a closing span");

        let mapped_span_id = self
            .registry_to_tracer_id_mapping
            .remove(&span_id)
            .expect("span id to exist if closed");
        let span = trace
            .spans
            .get_mut(&mapped_span_id)
            .expect("span to exist in if closing");
        span.close_refreshing_duration(now_nanos_u64());
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
        let mapped_trace_id = self
            .registry_to_tracer_id_mapping
            .get(&trace_id)
            .expect("trace id to exist");
        let mapped_span_id = if trace_id == span_id {
            ROOT_SPAN_ID
        } else {
            *self
                .registry_to_tracer_id_mapping
                .get(&span_id)
                .expect("span id to exist")
        };
        let trace = self
            .traces
            .get_mut(mapped_trace_id)
            .expect("trace to exist if it has a new span");
        trace.events_produced += 1;
        trace.new_events.push(SpanEvent {
            span_id: mapped_span_id,
            message,
            timestamp: now_nanos_u64(),
            severity,
            key_vals,
            location,
        });
    }
    pub fn insert_event_dropped_by_sampling(&mut self, trace_id: Id) {
        let mapped_trace_id = self
            .registry_to_tracer_id_mapping
            .get(&trace_id)
            .expect("trace id to exist");
        let trace = self
            .traces
            .get_mut(mapped_trace_id)
            .expect("trace to exist if it has an event dropped by sampling");
        trace.events_produced += 1;
        trace.events_dropped_by_sampling += 1;
    }
    pub fn insert_orphan_event(&mut self, event: OrphanEvent) {
        self.orphan_events.push(event);
    }
}
