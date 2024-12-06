use crate::subscriber::state::{TraceStateWithSpanCount, TracesAndOrphanEvents};
use api_structs::instance::update::{Event, Location, Span, TraceSnapshot, ROOT_SPAN_ID};
use api_structs::time_conversion::now_nanos_u64;
use api_structs::{Severity, SpanId, TraceId};
use deepsize::DeepSizeOf;
use maplit::hashmap;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DataInTracerFormatTrackingStorage {
    orphan_events: Vec<Event>,
    traces: HashMap<u64, TraceStateWithSpanCount>,
    /// used to generate sequential trace ids
    trace_count: u64,
}

impl DataInTracerFormatTrackingStorage {
    pub fn new() -> Self {
        Self {
            orphan_events: vec![],
            traces: HashMap::new(),
            trace_count: 0,
        }
    }
    pub fn size_bytes(&self) -> u64 {
        let trace_size: u64 = self
            .traces
            .values()
            .map(|e| e.trace_snapshot.deep_size_of() as u64)
            .sum();
        let orphan_events_size: u64 = self
            .orphan_events
            .iter()
            .map(|e| e.deep_size_of() as u64)
            .sum();
        trace_size + orphan_events_size
    }
    pub fn insert_new_trace(
        &mut self,
        name: String,
        // key_vals:  HashMap<String, String>,
        key_vals: HashMap<String, serde_json::Value>,
        location: Location,
    ) -> TraceId {
        let trace_id: TraceId = self.trace_count;
        self.trace_count += 1;
        let span = Span {
            id: ROOT_SPAN_ID,
            name,
            created_at_timestamp: now_nanos_u64(),
            duration: 0,
            events: vec![],
            parent_id: None,
            attributes: key_vals,
            location,
            links_to: None,
            is_closed: false,
        };
        let new_trace = TraceSnapshot {
            trace_id: trace_id,
            spans: hashmap! {span.id => span},
        };
        self.insert_trace(new_trace);
        trace_id
    }
    pub fn insert_new_span(
        &mut self,
        trace_id: TraceId,
        parent_id: SpanId,
        name: String,
        key_vals: HashMap<String, serde_json::Value>,
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
            created_at_timestamp: now_nanos_u64(),
            duration: 0,
            events: vec![],
            parent_id: Some(parent_id),
            attributes: key_vals,
            location,
            links_to: None,
            is_closed: false,
        };
        Self::insert_span(trace, new_span);
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
        self.orphan_events.push(orphan_event);
    }

    pub fn extract_data_for_export_pruning_internally(&mut self) -> TracesAndOrphanEvents {
        let total_size_bytes = self.size_bytes();
        let orphan_events = self.remove_all_orphan_events();
        let traces = self.traces.clone();
        let mut trace_ids_to_remove = vec![];
        let mut trace_spans_to_remove: Vec<(TraceId, SpanId)> = vec![];
        let mut trace_spans_to_prune: Vec<(TraceId, SpanId)> = vec![];
        for trace in &mut self.traces.values_mut() {
            if trace.trace_snapshot.is_closed() {
                trace_ids_to_remove.push(trace.trace_snapshot.trace_id);
            } else {
                for span in trace.trace_snapshot.spans.values_mut() {
                    if span.is_closed {
                        trace_spans_to_remove.push((trace.trace_snapshot.trace_id, span.id));
                    } else {
                        trace_spans_to_prune.push((trace.trace_snapshot.trace_id, span.id));
                    }
                }
            }
        }
        for trace_id in trace_ids_to_remove {
            self.remove_trace(trace_id);
        }
        for (t, s) in trace_spans_to_remove {
            self.remove_span(t, s);
        }
        for (t, s) in trace_spans_to_prune {
            self.prune_span_truncating_events_and_key_vals(t, s);
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

    fn insert_trace(&mut self, new_trace: TraceSnapshot) {
        let span_count = new_trace.spans.len() as u64;
        assert_eq!(span_count, 1, "new trace should have 1 span");
        let existing = self.traces.insert(
            new_trace.trace_id,
            TraceStateWithSpanCount {
                trace_snapshot: new_trace,
                span_count,
            },
        );
        assert!(existing.is_none());
    }
    fn remove_span(&mut self, trace_id: TraceId, span_id: SpanId) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace should exist if being removed");
        trace
            .trace_snapshot
            .spans
            .remove(&span_id)
            .expect("span being removed to exist");
    }
    fn prune_span_truncating_events_and_key_vals(&mut self, trace_id: TraceId, span_id: SpanId) {
        let trace = self
            .traces
            .get_mut(&trace_id)
            .expect("trace should exist if being removed");
        let span = trace
            .trace_snapshot
            .spans
            .get_mut(&span_id)
            .expect("span being pruned to exist");
        let _events = std::mem::take(&mut span.events);
        let _key_vals = std::mem::take(&mut span.attributes);
    }
    fn remove_trace(&mut self, trace_id: TraceId) {
        let _trace = self
            .traces
            .remove(&trace_id)
            .expect("trace should exist if being removed");
    }

    pub fn remove_all_orphan_events(&mut self) -> Vec<Event> {
        let events = std::mem::take(&mut self.orphan_events);
        events
    }

    fn insert_span(trace: &mut TraceStateWithSpanCount, span: Span) {
        let existing = trace.trace_snapshot.spans.insert(span.id, span);
        assert!(existing.is_none());
    }
    pub fn add_attributes_to_span(
        &mut self,
        trace_id: TraceId,
        span_id: SpanId,
        attributes: HashMap<String, serde_json::Value>,
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
        span.attributes.extend(attributes);
    }
    pub fn create_and_insert_span_event(
        &mut self,
        trace_id: TraceId,
        span_id: SpanId,
        message: Option<String>,
        severity: Severity,
        key_vals: HashMap<String, serde_json::Value>,
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
            attributes: key_vals,
            location,
        };
        Self::insert_span_event(span, span_event);
    }

    fn insert_span_event(span: &mut Span, span_event: Event) {
        span.events.push(span_event);
    }
}

#[cfg(test)]
mod tests {
    use crate::subscriber::state::tracer_storage::DataInTracerFormatTrackingStorage;
    use api_structs::instance::update::Location;
    use maplit::hashmap;
    use std::collections::HashMap;

    #[test]
    fn a() {
        let mut data_in_tracer_format = DataInTracerFormatTrackingStorage::new();
        assert_eq!(data_in_tracer_format.size_bytes(), 0);
        let k_vals = hashmap! {"key".to_string()=>"value".to_string()};
        let location = Location {
            module: None,
            filename: None,
            line: None,
        };
        let trace_id = data_in_tracer_format.insert_new_trace(
            "test".to_string(),
            k_vals.clone(),
            location.clone(),
        );
        assert_eq!(data_in_tracer_format.size_bytes(), 908);
        data_in_tracer_format.extract_data_for_export_pruning_internally();
        assert_eq!(data_in_tracer_format.size_bytes(), 756);
        data_in_tracer_format.extract_data_for_export_pruning_internally();
        assert_eq!(data_in_tracer_format.size_bytes(), 756);
        data_in_tracer_format.insert_new_span(
            trace_id,
            trace_id,
            "somea".to_string(),
            k_vals,
            location.clone(),
        );
        assert_eq!(data_in_tracer_format.size_bytes(), 913);
        data_in_tracer_format.extract_data_for_export_pruning_internally();
        assert_eq!(data_in_tracer_format.size_bytes(), 761);
        let span_id = data_in_tracer_format.insert_new_span(
            trace_id,
            trace_id,
            "somea".to_string(),
            HashMap::new(),
            location.clone(),
        );
        assert_eq!(data_in_tracer_format.size_bytes(), 766);
        data_in_tracer_format.extract_data_for_export_pruning_internally();
        assert_eq!(data_in_tracer_format.size_bytes(), 766);
        data_in_tracer_format.close_span(trace_id, span_id);
        assert_eq!(data_in_tracer_format.size_bytes(), 766);
        data_in_tracer_format.extract_data_for_export_pruning_internally();
        assert_eq!(data_in_tracer_format.size_bytes(), 761);
    }
}
