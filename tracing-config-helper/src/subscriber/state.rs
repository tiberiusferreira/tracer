use deepsize::DeepSizeOf;
use maplit::hashmap;
use std::collections::HashMap;
use tracing::Id;

use api_structs::instance::update::{Event, Location, Span, TraceSnapshot, ROOT_SPAN_ID};
use api_structs::time_conversion::now_nanos_u64;
use api_structs::{Severity, SpanId, TraceId};

#[derive(Debug, Clone)]
struct TraceStateWithSpanCount {
    trace_snapshot: TraceSnapshot,
    span_count: u64,
}

#[derive(Debug, Clone)]
struct DataInTracerFormatTrackingStorage {
    orphan_events: Vec<Event>,
    traces: HashMap<u64, TraceStateWithSpanCount>,
    /// used to generate sequential trace ids
    trace_count: u64,
    total_size_bytes: u64,
}

impl DataInTracerFormatTrackingStorage {
    pub fn new() -> Self {
        Self {
            orphan_events: vec![],
            traces: HashMap::new(),
            trace_count: 0,
            total_size_bytes: 0,
        }
    }
    pub fn insert_new_trace(
        &mut self,
        name: String,
        key_vals: HashMap<String, String>,
        location: Location,
    ) -> TraceId {
        let trace_id: TraceId = self.trace_count;
        self.trace_count += 1;
        let span = Span {
            id: ROOT_SPAN_ID,
            name,
            timestamp: now_nanos_u64(),
            duration: 0,
            events: vec![],
            parent_id: None,
            key_vals,
            location,
            links_to: None,
            is_closed: false,
        };
        let new_trace = TraceSnapshot {
            id: trace_id,
            spans: hashmap! {span.id => span},
        };
        self.insert_trace_updating_size(new_trace);
        trace_id
    }
    pub fn insert_new_span(
        &mut self,
        trace_id: TraceId,
        parent_id: SpanId,
        name: String,
        key_vals: HashMap<String, String>,
        location: Location,
    ) -> SpanId {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace to exist if it has a new span");
        let new_span_id = trace.span_count;
        trace.span_count += 1;
        assert!(
            trace.trace_snapshot.spans.get(&parent_id).is_some(),
            "span parent must be valid"
        );
        let new_span = Span {
            id: new_span_id,
            name,
            timestamp: now_nanos_u64(),
            duration: 0,
            events: vec![],
            parent_id: Some(parent_id),
            key_vals,
            location,
            links_to: None,
            is_closed: false,
        };
        Self::insert_span_updating_size(&mut self.total_size_bytes, trace, new_span);
        new_span_id
    }
    pub fn close_span(&mut self, trace_id: TraceId, span_id: SpanId) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace to exist if it has a closing span");

        let span = trace
            .trace_snapshot
            .spans
            .get_mut(&span_id)
            .expect("span to exist in if closing");
        span.close_refreshing_duration(now_nanos_u64());
    }
    pub fn close_trace(&mut self, trace_id: TraceId) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace to exist when closing");
        let root = trace.trace_snapshot.root_mut();
        root.close_refreshing_duration(now_nanos_u64());
        if trace.trace_snapshot.spans.values().any(|s| !s.is_closed) {
            panic!("trace closed before all spans had closed!");
        }
    }

    pub fn insert_orphan_event(&mut self, orphan_event: Event) {
        let new_bytes = orphan_event.deep_size_of();
        self.total_size_bytes += new_bytes as u64;
        self.orphan_events.push(orphan_event);
    }

    pub fn extract_data_for_export_pruning_internally(&mut self) -> TracesAndOrphanEvents {
        let total_size_bytes = self.total_size_bytes;
        let orphan_events = self.remove_all_orphan_events();
        let traces = self.traces.clone();
        let mut trace_ids_to_remove = vec![];
        let mut trace_spans_to_remove: Vec<(TraceId, SpanId)> = vec![];
        let mut trace_spans_to_prune: Vec<(TraceId, SpanId)> = vec![];
        for trace in &mut self.traces.values_mut() {
            if trace.trace_snapshot.is_closed() {
                trace_ids_to_remove.push(trace.trace_snapshot.id);
            } else {
                for span in trace.trace_snapshot.spans.values_mut() {
                    if span.is_closed {
                        trace_spans_to_remove.push((trace.trace_snapshot.id, span.id));
                    } else {
                        trace_spans_to_prune.push((trace.trace_snapshot.id, span.id));
                    }
                }
            }
        }
        for trace_id in trace_ids_to_remove {
            self.remove_trace_updating_size(trace_id);
        }
        for (t, s) in trace_spans_to_remove {
            self.remove_span_updating_size(t, s);
        }
        for (t, s) in trace_spans_to_prune {
            self.prune_span_truncating_events_and_key_vals_updating_size(t, s);
        }
        TracesAndOrphanEvents {
            orphan_events,
            traces: traces
                .into_iter()
                .map(|(id, t)| (id, t.trace_snapshot))
                .collect(),
            export_buffer_size_bytes: total_size_bytes,
        }
    }

    fn insert_trace_updating_size(&mut self, new_trace: TraceSnapshot) {
        let new_bytes = new_trace.deep_size_of();
        let span_count = new_trace.spans.len() as u64;
        assert_eq!(span_count, 1, "new trace should have 1 span");
        let existing = self.traces.insert(
            new_trace.id,
            TraceStateWithSpanCount {
                trace_snapshot: new_trace,
                span_count,
            },
        );
        assert!(existing.is_none());
        self.total_size_bytes += new_bytes as u64;
    }
    fn remove_span_updating_size(&mut self, trace_id: TraceId, span_id: SpanId) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace should exist if being removed");
        let span = trace
            .trace_snapshot
            .spans
            .remove(&span_id)
            .expect("span being removed to exist");
        let old_bytes = span.deep_size_of();
        self.total_size_bytes -= old_bytes as u64;
    }
    fn prune_span_truncating_events_and_key_vals_updating_size(
        &mut self,
        trace_id: TraceId,
        span_id: SpanId,
    ) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace should exist if being removed");
        let span = trace
            .trace_snapshot
            .spans
            .get_mut(&span_id)
            .expect("span being pruned to exist");
        let events = std::mem::take(&mut span.events);
        let key_vals = std::mem::take(&mut span.key_vals);
        let events_old_bytes = events.deep_size_of();
        let key_vals_old_bytes = key_vals.deep_size_of();
        self.total_size_bytes -= events_old_bytes as u64;
        self.total_size_bytes -= key_vals_old_bytes as u64;
    }
    fn remove_trace_updating_size(&mut self, trace_id: TraceId) {
        let trace = self
            .traces
            .remove(&trace_id)
            .expect("trace should exist if being removed")
            .trace_snapshot;
        let old_bytes = trace.deep_size_of();
        self.total_size_bytes -= old_bytes as u64;
    }

    pub fn remove_all_orphan_events(&mut self) -> Vec<Event> {
        let events = std::mem::take(&mut self.orphan_events);
        let old_bytes: u64 = events.iter().map(|e| e.deep_size_of() as u64).sum();
        self.total_size_bytes -= old_bytes;
        events
    }

    fn insert_span_updating_size(
        total_size_bytes: &mut u64,
        trace: &mut TraceStateWithSpanCount,
        span: Span,
    ) {
        let new_bytes = span.deep_size_of();
        let existing = trace.trace_snapshot.spans.insert(span.id, span);
        assert!(existing.is_none());
        *total_size_bytes += new_bytes as u64;
    }

    pub fn insert_span_event(
        &mut self,
        trace_id: TraceId,
        span_id: SpanId,
        message: Option<String>,
        severity: Severity,
        key_vals: HashMap<String, String>,
        location: Location,
    ) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace to exist if it has a new span");
        let span = trace
            .trace_snapshot
            .spans
            .get_mut(&span_id)
            .expect("span to exist is adding an event to it");
        let span_event = Event {
            message,
            timestamp: now_nanos_u64(),
            severity,
            key_vals,
            location,
        };
        Self::insert_span_event_updating_size(span, span_event, &mut self.total_size_bytes);
    }

    fn insert_span_event_updating_size(
        span: &mut Span,
        span_event: Event,
        total_size_bytes: &mut u64,
    ) {
        let new_bytes = span_event.deep_size_of();
        span.events.push(span_event);
        *total_size_bytes += new_bytes as u64;
    }
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
        let tracer_parent_id = self
            .registry_to_tracer_id_mapping
            .get(&parent_id)
            .expect("parent span id to exist");
        let tracer_new_span_id = self.data.insert_new_span(
            *tracer_trace_id,
            *tracer_parent_id,
            name,
            key_vals,
            location,
        );
        self.registry_to_tracer_id_mapping
            .insert(span_id.clone(), tracer_new_span_id);
    }
    pub fn close_span(&mut self, trace_id: Id, span_id: Id) {
        let tracer_trace_id = *self
            .registry_to_tracer_id_mapping
            .get(&trace_id)
            .expect("trace id to exist if has closing span");
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
        let tracer_span_id = *self
            .registry_to_tracer_id_mapping
            .get(&span_id)
            .expect("span id to exist if has new event");
        self.data.insert_span_event(
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
