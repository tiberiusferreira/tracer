use crate::print_if_dbg;
use crate::subscriber::attribute_visitor::AttributesVisitor;
use crate::subscriber::state::{State, TracesAndOrphanEvents};
use api_structs::instance::update::{Location, Severity};
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
    state: Arc<parking_lot::RwLock<State>>,
}

pub struct ExportDataGetter(Arc<parking_lot::RwLock<State>>);
impl ExportDataGetter {
    pub fn get_data_ready_to_export(&self) -> TracesAndOrphanEvents {
        self.0.write().remove_data_ready_to_export()
    }
}

impl TracerTracingSubscriber {
    pub fn new() -> Self {
        let tracer = Self {
            state: Arc::new(parking_lot::RwLock::new(State::new())),
        };
        tracer
    }

    pub fn export_data_getter_handle(&self) -> ExportDataGetter {
        ExportDataGetter(Arc::clone(&self.state))
    }

    fn span_root<'a, S: Subscriber + for<'b> LookupSpan<'b>>(
        span_id: Id,
        ctx: &'a Context<S>,
    ) -> Option<SpanRef<'a, S>> {
        let root = ctx.span(&span_id)?.scope().from_root().next()?;
        Some(root)
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
        let root_span = Self::span_root(id.clone(), &ctx).expect("root span to exist");
        let span = ctx.span(id).expect("created span to exist!");
        let location = location_from_metadata(span.metadata());
        let mut attributes_visitor = AttributesVisitor::new();
        attrs.record(&mut attributes_visitor);
        let key_vals = attributes_visitor.key_vals;
        print_if_dbg(
            context,
            format!("span {} had {:#?} key-val", span.name(), key_vals),
        );
        if root_span.id() == *id {
            self.state
                .write()
                .insert_new_trace(id, span.name().to_string(), key_vals, location);
        } else {
            self.state.write().insert_new_span(
                root_span.id(),
                span.id(),
                span.parent().expect("non root span to have a parent").id(),
                span.name().to_string(),
                key_vals,
                location,
            );
        }
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
                self.state
                    .write()
                    .insert_orphan_event(api_structs::instance::update::Event {
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

        let root = Self::span_root(span.id(), &ctx).expect("root span to exist");
        print_if_dbg(context, "Allowed by sampler, sending to exporter.");
        self.state.write().insert_span_event(
            root.id(),
            span.id(),
            event_data.message,
            event_data.level,
            event_data.key_vals,
            location_from_metadata(event.metadata()),
        );
    }

    fn on_enter(&self, id: &Id, _ctx: Context<'_, S>) {
        let context = "on_enter";
        print_if_dbg(context, format!("on enter for {id:?}"));
    }

    fn on_close(&self, span_id: Id, ctx: Context<'_, S>) {
        let context = "on_close";
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
            self.state.write().close_trace(root_span_id.clone());
        } else {
            print_if_dbg(
                context,
                format!(
                    "Span {} was not trace root, closing span",
                    span_id.into_u64()
                ),
            );
            self.state.write().close_span(root_span_id.clone(), span_id);
        }
    }
}
