use crate::print_if_dbg;
use crate::subscriber::attribute_visitor::AttributesVisitor;
use crate::subscriber::state::{State, TracesAndOrphanEvents};
use api_structs::instance::update::{Location, OrphanEvent, Sampling, Severity};
use api_structs::time_conversion::now_nanos_u64;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::span::{Attributes, Record};
use tracing::{Event, Id, Metadata, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::{LookupSpan, SpanRef};
use tracing_subscriber::Layer;

pub mod attribute_visitor;
// pub mod sampler;
pub mod state;

/// The subscriber:
/// Receives new spans and events
/// Handles span renaming if needed
/// Checks if they should be kept or not by asking the Sampler
/// Formats the data in a more ergonomic structure and passes it on to the export buffer
pub struct TracerTracingSubscriber {
    exporter_state: Arc<parking_lot::RwLock<State>>,
}

pub struct ExporterStateHandle(Arc<parking_lot::RwLock<State>>);
impl ExporterStateHandle {
    pub fn get_export_data(&self) -> TracesAndOrphanEvents {
        self.0.write().get_export_data()
    }
}

impl TracerTracingSubscriber {
    pub fn new() -> Self {
        let tracer = Self {
            exporter_state: Arc::new(parking_lot::RwLock::new(State::new())),
        };
        tracer
    }

    pub fn get_sampler_state_handle(&self) -> ExporterStateHandle {
        ExporterStateHandle(Arc::clone(&self.exporter_state))
    }

    fn create_tracer_span_data_with_key_vals_and_final_name<
        S: Subscriber + for<'a> LookupSpan<'a>,
    >(
        span: &SpanRef<S>,
        mut key_vals: HashMap<String, String>,
    ) {
        let context = "create_tracer_span_data_with_key_vals_and_final_name";
        let mut extensions = span.extensions_mut();
        let name = span.name().to_string();
        extensions.insert(TracerSpanData {
            key_vals,
            name,
            first_entered_at: None,
        });
    }

    fn take_tracer_span_data_key_vals<S: Subscriber + for<'a> LookupSpan<'a>>(
        span: &SpanRef<S>,
    ) -> HashMap<String, String> {
        let mut extensions = span.extensions_mut();
        let tracer_span_data: &mut TracerSpanData = extensions
            .get_mut()
            .expect("tracer_span_data to exist when take_tracer_span_data_key_vals is called");
        std::mem::take(&mut tracer_span_data.key_vals)
    }

    fn set_span_as_entered<S: Subscriber + for<'a> LookupSpan<'a>>(span: &SpanRef<S>) {
        let mut extensions = span.extensions_mut();
        let tracer_span_data: &mut TracerSpanData = extensions
            .get_mut()
            .expect("tracer_span_data to exist when set_span_as_entered is called");
        tracer_span_data.first_entered_at = Some(std::time::Instant::now());
    }

    fn span_was_already_entered<S: Subscriber + for<'a> LookupSpan<'a>>(span: &SpanRef<S>) -> bool {
        let mut extensions = span.extensions_mut();
        let tracer_span_data: &mut TracerSpanData = extensions
            .get_mut()
            .expect("tracer_span_data to exist when span_was_already_entered is called");
        return tracer_span_data.first_entered_at.is_some();
    }
    fn span_root<'a, S: Subscriber + for<'b> LookupSpan<'b>>(
        span_id: Id,
        ctx: &'a Context<S>,
    ) -> Option<SpanRef<'a, S>> {
        let root = ctx.span(&span_id)?.scope().from_root().next()?;
        Some(root)
    }
    fn span_final_name<'a, S: Subscriber + for<'b> LookupSpan<'b>>(span: &SpanRef<S>) -> String {
        let extensions = span.extensions();
        let tracer_span_data: &TracerSpanData = extensions
            .get()
            .expect("tracer_span_data to exist when span_final_name is called");
        return tracer_span_data.name.to_string();
    }

    fn trace_was_dropped<'a, S: Subscriber + for<'b> LookupSpan<'b>>(
        span_id: Id,
        ctx: &'a Context<S>,
    ) -> bool {
        let root_span = Self::span_root(span_id, &ctx).expect("root to exist even if itself");
        let root_extensions = root_span.extensions();
        let tracer_root_span_data: &TracerRootSpanData = root_extensions
            .get()
            .expect("root span to have TracerRootSpanData");
        tracer_root_span_data.dropped
    }

    fn extract_event_information(event: &Event) -> EventData {
        let mut event_visitor = AttributesVisitor::new();
        event.record(&mut event_visitor);
        let level = match event.metadata().level() {
            &tracing::metadata::Level::TRACE => Severity::Trace,
            &tracing::metadata::Level::DEBUG => Severity::Debug,
            &tracing::metadata::Level::INFO => Severity::Info,
            &tracing::metadata::Level::WARN => Severity::Warn,
            &tracing::metadata::Level::ERROR => Severity::Error,
        };
        EventData {
            message: event_visitor.message,
            level,
            key_vals: event_visitor.key_vals,
        }
    }
}

/// This is our custom data, created and attached to every single span when it is created
/// and then the first entered is added on enter
struct TracerSpanData {
    key_vals: HashMap<String, String>,
    name: String,
    // used to check if span was already entered
    first_entered_at: Option<std::time::Instant>,
}

/// This is another piece of custom data, but only created and attached to Root Spans.
/// We use this to detect traces that were dropped
#[derive(Clone, Debug)]
struct TracerRootSpanData {
    dropped: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventData {
    pub message: Option<String>,
    pub level: Severity,
    pub key_vals: HashMap<String, String>,
}

fn location_from_metadata(metadata: &Metadata) -> Location {
    Location {
        module: metadata.module_path().map(|e| e.to_string()),
        filename: metadata.file().map(|e| e.to_string()),
        line: metadata.line(),
    }
}
impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for TracerTracingSubscriber {
    /// We only export spans once they are entered, so here we store the key_values and
    /// proper span name for using when first entered
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let context = "on_new_span";
        let span = ctx.span(id).expect("new span to exist!");
        let mut attributes_visitor = AttributesVisitor::new();
        attrs.record(&mut attributes_visitor);
        print_if_dbg(
            context,
            format!(
                "span {} had {} key-val",
                span.name(),
                attributes_visitor.key_vals.len()
            ),
        );
        Self::create_tracer_span_data_with_key_vals_and_final_name(
            &span,
            attributes_visitor.key_vals,
        );
    }
    fn on_record(&self, _span: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {
        let context = "on_record";
        print_if_dbg(context, "on record");
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let context = "on_event";
        let span = ctx.event_span(event);
        let event_data = Self::extract_event_information(event);

        let span = match span {
            None => {
                print_if_dbg(context, "Event is orphan");
                print_if_dbg(context, "Sending to exporter");
                self.exporter_state
                    .write()
                    .insert_orphan_event(OrphanEvent {
                        message: event_data.message,
                        timestamp: now_nanos_u64(),
                        severity: event_data.level,
                        key_vals: event_data.key_vals,
                        location: location_from_metadata(event.metadata()),
                    });
                return;
            }
            Some(span) => {
                print_if_dbg(context, "Event belongs to a span");
                span
            }
        };

        if Self::trace_was_dropped(span.id(), &ctx) {
            print_if_dbg(
                context,
                "Event belongs to trace previously dropped, dropping event.",
            );
            return;
        }
        let root = Self::span_root(span.id(), &ctx).expect("root span to exist");
        print_if_dbg(context, "Allowed by sampler, sending to exporter.");
        self.exporter_state.write().insert_span_event(
            root.id(),
            span.id(),
            event_data.message,
            event_data.level,
            event_data.key_vals,
            location_from_metadata(event.metadata()),
        );
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let context = "on_enter";
        let span = ctx.span(id).expect("entered span to exist!");
        let span_name = Self::span_final_name(&span);
        if Self::span_was_already_entered(&span) {
            print_if_dbg(context, format!("span {span_name} entered again"));
            return;
        } else {
            print_if_dbg(
                context,
                format!("span {span_name} entered for the first time"),
            );
            Self::set_span_as_entered(&span);
        }
        let root_span = Self::span_root(id.clone(), &ctx).expect("root span to exist");
        let root_span_name = Self::span_final_name(&root_span);
        // if span and root_span are the same, we are the root span, so a new trace is being born here
        if root_span.id() == *id {
            print_if_dbg(
                context,
                format!(
                    "Span is root. Name: {} Id: {}",
                    root_span_name,
                    id.into_u64()
                ),
            );
            // check is this new trace is not over the limit
            let new_trace_allowed = true;
            if new_trace_allowed {
                print_if_dbg(context, "Allowed by sampler, sending to exporter");
                let key_vals = Self::take_tracer_span_data_key_vals(&span);
                self.exporter_state.write().insert_new_trace(
                    id,
                    span_name,
                    key_vals,
                    location_from_metadata(root_span.metadata()),
                );
                root_span
                    .extensions_mut()
                    .insert(TracerRootSpanData { dropped: false })
            } else {
                print_if_dbg(context, "Not Allowed by sampler");
                root_span
                    .extensions_mut()
                    .insert(TracerRootSpanData { dropped: true })
            }
        } else {
            // we are not root, check if current trace was dropped
            print_if_dbg(context, format!("New non-root span. Id: {}", id.into_u64()));
            if Self::trace_was_dropped(id.clone(), &ctx) {
                print_if_dbg(
                    context,
                    "Span belongs to previously dropped trace, dropping it.",
                );
                return;
            } else {
                print_if_dbg(context, "Span belongs to non-dropped trace");
                let new_span_kv_allowed = true;
                let key_vals = Self::take_tracer_span_data_key_vals(&span);
                let key_vals = if new_span_kv_allowed {
                    print_if_dbg(context, "KV allowed by sampler");
                    key_vals
                } else {
                    print_if_dbg(context, "KV not allowed by sampler");
                    HashMap::new()
                };
                let parent_id = span.parent().expect("parent to exist if non-root").id();
                self.exporter_state.write().insert_new_span(
                    root_span.id(),
                    id.clone(),
                    parent_id,
                    span_name.to_string(),
                    key_vals,
                    location_from_metadata(span.metadata()),
                );
            }
        }
    }
    fn on_close(&self, span_id: Id, ctx: Context<'_, S>) {
        let context = "on_close";
        let span = ctx.span(&span_id).expect("span to exist if it got closed");
        if Self::trace_was_dropped(span.id(), &ctx) {
            print_if_dbg(
                context,
                "Span belongs to previously dropped trace, dropping it",
            );
            return;
        }
        let root_span_id = Self::span_root(span_id.clone(), &ctx)
            .expect("root span to exist")
            .id();

        print_if_dbg(
            context,
            format!("Span {} closed. Sending to exporter", span_id.into_u64()),
        );
        if root_span_id == span_id {
            print_if_dbg(
                context,
                format!("Span {} was trace root, closing trace", span_id.into_u64()),
            );
            self.exporter_state
                .write()
                .close_trace(root_span_id.clone());
        } else {
            print_if_dbg(
                context,
                format!(
                    "Span {} was not trace root, closing span",
                    span_id.into_u64()
                ),
            );
            self.exporter_state
                .write()
                .close_span(root_span_id.clone(), span_id);
        }
    }
}
