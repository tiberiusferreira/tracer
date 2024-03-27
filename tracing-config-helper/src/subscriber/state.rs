use std::collections::HashMap;

use api_structs::instance::update::{
    ClosedSpan, NewOrphanEvent, NewSpanEvent, OpenSpan, RootSpan, TraceState,
};

#[derive(Debug, Clone)]
pub struct State {
    traces: HashMap<u64, TraceState>,
    orphan_events: Vec<NewOrphanEvent>,
}

#[derive(Debug, Clone)]
pub struct TracesAndOrphanEvents {
    pub traces: HashMap<u64, TraceState>,
    pub orphan_events: Vec<NewOrphanEvent>,
}

impl State {
    pub fn new() -> Self {
        Self {
            traces: HashMap::new(),
            orphan_events: vec![],
        }
    }
    pub fn get_export_data(&mut self) -> TracesAndOrphanEvents {
        let orphan_events = std::mem::take(&mut self.orphan_events);
        let traces = self.traces.clone();
        // only retain traces still running
        self.traces.retain(|_k, v| !v.is_closed());
        for trace in self.traces.values_mut() {
            trace.closed_spans.clear();
            trace.new_events.clear();
        }
        TracesAndOrphanEvents {
            traces,
            orphan_events,
        }
    }
    pub fn insert_new_trace(&mut self, root_span: RootSpan) {
        let existing = self.traces.insert(
            root_span.id,
            TraceState {
                root_span,
                open_spans: HashMap::new(),
                spans_produced: 1,
                events_produced: 0,
                events_dropped_by_sampling: 0,
                closed_spans: vec![],
                new_events: vec![],
            },
        );
        assert!(existing.is_none());
    }
    pub fn close_trace(&mut self, trace_id: u64, duration: u64) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace to exist if it has a new span");
        trace.root_span.duration = Some(duration);
        assert!(
            trace.open_spans.is_empty(),
            "when the trace gets closed, all its children spans should also already be"
        );
    }
    pub fn insert_new_span(&mut self, trace_id: u64, open_span: OpenSpan) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace to exist if it has a new span");
        let existing = trace.open_spans.insert(open_span.id, open_span);
        trace.spans_produced += 1;
        assert!(existing.is_none());
    }
    pub fn close_span(&mut self, trace_id: u64, span_id: u64, duration: u64) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace to exist if it has a new span");
        let span = trace
            .open_spans
            .remove(&span_id)
            .expect("span to exist in open_spans if closed");
        trace.closed_spans.push(ClosedSpan {
            id: span.id,
            name: span.name,
            timestamp: span.timestamp,
            duration,
            parent_id: span.parent_id,
            key_vals: span.key_vals,
            location: span.location,
        });
    }

    pub fn insert_span_event(&mut self, trace_id: u64, event: NewSpanEvent) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace to exist if it has a new span");
        // if trace_id == event.span_id, this event is for root
        if trace_id != event.span_id {
            assert!(
                trace.open_spans.get(&event.span_id).is_some(),
                "tried to insert an event for a non-existing span trace: {trace_id} event: {event:?}"
            );
        }
        trace.events_produced += 1;
        trace.new_events.push(event);
    }
    pub fn insert_event_dropped_by_sampling(&mut self, trace_id: u64) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace to exist if it has a new span");
        trace.events_produced += 1;
        trace.events_dropped_by_sampling += 1;
    }
    pub fn insert_orphan_event(&mut self, event: NewOrphanEvent) {
        self.orphan_events.push(event);
    }
}
