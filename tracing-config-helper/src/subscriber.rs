use crate::sampling::{Sampler, TracerSampler};
use crate::{print_if_dbg, NewSpan, NewSpanEvent, SubscriberEvent, TracerTracingSubscriber};
use api_structs::exporter::status::SamplerLimits;
use api_structs::exporter::trace_exporting::{
    ClosedSpan, NewOrphanEvent, Severity, SpanEventCount,
};
use api_structs::time_conversion::now_nanos_u64;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Record};
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::{LookupSpan, SpanRef};
use tracing_subscriber::Layer;

impl TracerTracingSubscriber {
    pub fn new(
        sampler_limits: SamplerLimits,
        subscriber_event_sender: Sender<SubscriberEvent>,
    ) -> Self {
        let sampler = Arc::new(parking_lot::RwLock::new(TracerSampler::new(sampler_limits)));
        let tracer = Self {
            sampler,
            subscriber_event_sender,
        };
        tracer
    }

    fn create_tracer_span_data_with_key_vals<S: Subscriber + for<'a> LookupSpan<'a>>(
        span: &SpanRef<S>,
        key_vals: HashMap<String, String>,
    ) {
        let mut extensions = span.extensions_mut();
        extensions.insert(TracerSpanData {
            key_vals,
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

    fn trace_timestamp<'a, S: Subscriber + for<'b> LookupSpan<'b>>(
        span_id: Id,
        ctx: &'a Context<S>,
    ) -> u64 {
        let root_span = Self::span_root(span_id, &ctx).expect("root to exist even if itself");
        let root_extensions = root_span.extensions();
        let tracer_root_span_data: &TracerRootSpanData = root_extensions
            .get()
            .expect("root span to have TracerRootSpanData");
        tracer_root_span_data.timestamp
    }

    fn increment_trace_span_count<'a, S: Subscriber + for<'b> LookupSpan<'b>>(
        span_id: Id,
        ctx: &'a Context<S>,
    ) -> SpanEventCount {
        let root_span =
            Self::span_root(span_id.clone(), &ctx).expect("root to exist even if itself");
        let mut root_extensions = root_span.extensions_mut();
        let tracer_root_span_data: &mut TracerRootSpanData = root_extensions
            .get_mut()
            .expect("root span to have TracerRootSpanData");
        tracer_root_span_data.span_count += 1;
        SpanEventCount {
            span_count: tracer_root_span_data.span_count,
            event_count: tracer_root_span_data.event_count,
        }
    }
    fn increment_trace_event_count<'a, S: Subscriber + for<'b> LookupSpan<'b>>(
        span_id: Id,
        ctx: &'a Context<S>,
    ) -> SpanEventCount {
        let root_span = Self::span_root(span_id, &ctx).expect("root to exist even if itself");
        let mut root_extensions = root_span.extensions_mut();
        let tracer_root_span_data: &mut TracerRootSpanData = root_extensions
            .get_mut()
            .expect("root span to have TracerRootSpanData");
        tracer_root_span_data.event_count += 1;
        SpanEventCount {
            span_count: tracer_root_span_data.span_count,
            event_count: tracer_root_span_data.event_count,
        }
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
            timestamp: now_nanos_u64(),
            level,
            key_vals: event_visitor.key_vals,
        }
    }
    // Tries to send the event to the export, dropping it if buffer is full
    fn send_subscriber_event_to_export(&self, subscriber_event: SubscriberEvent) {
        let context = "send_subscriber_event_to_export";
        match self
            .subscriber_event_sender
            .try_send(subscriber_event.clone())
        {
            Ok(_) => {
                print_if_dbg(context, format!("Send event {:#?}", subscriber_event));
            }
            Err(_e) => {
                print_if_dbg(
                    context,
                    format!("Send failed for event {:#?}", subscriber_event),
                );
                self.sampler
                    .write()
                    .register_soe_dropped_due_to_full_export_buffer();
            }
        }
    }
}

struct AttributesVisitor {
    pub message: Option<String>,
    pub key_vals: HashMap<String, String>,
}

impl AttributesVisitor {
    pub fn new() -> Self {
        Self {
            message: None,
            key_vals: HashMap::new(),
        }
    }
}
impl Visit for AttributesVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        let context = "record_str";
        let key = field.name();
        print_if_dbg(context, format!("Got {} - {:?}", key, value));
        if key == "message" {
            self.message = Some(value.to_string());
        } else {
            self.key_vals.insert(key.to_string(), value.to_string());
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let context = "record_debug";
        let key = field.name();
        let val = format!("{:?}", value);
        print_if_dbg(context, format!("Got {} - {:?}", key, value));
        if key == "message" {
            self.message = Some(val);
        } else {
            self.key_vals.insert(key.to_string(), val);
        }
    }
}

/// This is our custom data, created and attached to every single span when it is created
/// and then the first entered is added on enter
struct TracerSpanData {
    key_vals: HashMap<String, String>,
    // we use this data to calculate the span duration when it gets closed
    first_entered_at: Option<std::time::Instant>,
}

/// This is another piece of custom data, but only created and attached to Root Spans.
/// We use this to detect traces that were dropped
#[derive(Clone, Debug)]
struct TracerRootSpanData {
    timestamp: u64,
    span_count: u32,
    event_count: u32,
    dropped: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventData {
    pub message: Option<String>,
    pub timestamp: u64,
    pub level: Severity,
    pub key_vals: HashMap<String, String>,
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for TracerTracingSubscriber {
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
                let new_orphan_event_allowed = {
                    let mut w_sampler = self.sampler.write();
                    w_sampler.allow_new_orphan_event()
                };
                return if new_orphan_event_allowed {
                    print_if_dbg(context, "Allowed by sampler, sending to exporter");
                    self.send_subscriber_event_to_export(SubscriberEvent::NewOrphanEvent(
                        NewOrphanEvent {
                            message: event_data.message,
                            timestamp: event_data.timestamp,
                            level: event_data.level,
                            key_vals: event_data.key_vals,
                        },
                    ));
                } else {
                    print_if_dbg(context, "Not Allowed by sampler, dropping");
                };
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
        let spe_count = Self::increment_trace_event_count(span.id(), &ctx);
        let new_event_allowed = {
            let mut w_sampler = self.sampler.write();
            w_sampler.allow_new_event(root.name())
        };
        if new_event_allowed {
            print_if_dbg(context, "Allowed by sampler, sending to exporter.");
            self.send_subscriber_event_to_export(SubscriberEvent::NewSpanEvent(NewSpanEvent {
                trace_id: root.id().into_non_zero_u64().get(),
                trace_name: root.name(),
                spe_count,
                trace_timestamp: Self::trace_timestamp(span.id(), &ctx),
                span_id: span.id().into_u64(),
                message: event_data.message,
                timestamp: event_data.timestamp,
                level: event_data.level,
                key_vals: event_data.key_vals,
            }));
        } else {
            print_if_dbg(
                context,
                format!("Not allowed by sampler, discarding event SpE.",),
            );
            self.send_subscriber_event_to_export(SubscriberEvent::SpanEventCountUpdate {
                trace_id: root.id().into_non_zero_u64().get(),
                trace_name: root.name(),
                trace_timestamp: Self::trace_timestamp(span.id(), &ctx),
                spe_count,
            });
        }
    }

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
        Self::create_tracer_span_data_with_key_vals(&span, attributes_visitor.key_vals);
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let context = "on_enter";
        let span = ctx.span(id).expect("entered span to exist!");
        let span_name = span.name();
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

        // if span and root_span are the same, we are the root span, so a new trace is being born here
        if root_span.id() == *id {
            print_if_dbg(context, format!("Span is root. Id: {}", id.into_u64()));
            // check is this new trace is not over the limit
            let new_trace_allowed = {
                let mut w_sampler = self.sampler.write();
                w_sampler.allow_new_trace(&root_span.name())
            };
            if new_trace_allowed {
                print_if_dbg(context, "Allowed by sampler, sending to exporter");
                let key_vals = Self::take_tracer_span_data_key_vals(&span);
                let now_nanos = now_nanos_u64();
                self.send_subscriber_event_to_export(SubscriberEvent::NewSpan(NewSpan {
                    id: id.into_u64(),
                    trace_id: id.into_non_zero_u64().get(),
                    name: span_name.to_string(),
                    parent_id: None,
                    timestamp: now_nanos,
                    key_vals,
                    trace_name: root_span.name(),
                    spe_count: SpanEventCount {
                        span_count: 1,
                        event_count: 0,
                    },
                    trace_timestamp: now_nanos,
                }));
                root_span.extensions_mut().insert(TracerRootSpanData {
                    timestamp: now_nanos,
                    span_count: 1,
                    event_count: 0,
                    dropped: false,
                })
            } else {
                print_if_dbg(context, "Not Allowed by sampler");
                root_span.extensions_mut().insert(TracerRootSpanData {
                    timestamp: 0,
                    span_count: 0,
                    event_count: 0,
                    dropped: true,
                })
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
                let spe_count = Self::increment_trace_span_count(id.clone(), &ctx);
                print_if_dbg(context, "Span belongs to non-dropped trace");
                let new_span_allowed = {
                    let mut w_sampler = self.sampler.write();
                    w_sampler.allow_new_span(root_span.name())
                };
                if new_span_allowed {
                    let key_vals = Self::take_tracer_span_data_key_vals(&span);
                    let parent_id = span.parent().expect("parent to exist if non-root").id();
                    print_if_dbg(context, "Allowed by sampler, sending to exporter");
                    self.send_subscriber_event_to_export(SubscriberEvent::NewSpan(NewSpan {
                        id: id.into_u64(),
                        trace_id: root_span.id().into_non_zero_u64().get(),
                        name: span_name.to_string(),
                        parent_id: Some(parent_id.into_u64()),
                        timestamp: now_nanos_u64(),
                        key_vals,
                        trace_name: root_span.name(),
                        spe_count,
                        trace_timestamp: Self::trace_timestamp(id.clone(), &ctx),
                    }));
                } else {
                    print_if_dbg(context, format!("Span Not Allowed by sampler"));
                    self.send_subscriber_event_to_export(SubscriberEvent::SpanEventCountUpdate {
                        trace_id: root_span.id().into_non_zero_u64().get(),
                        trace_name: root_span.name(),
                        trace_timestamp: Self::trace_timestamp(span.id(), &ctx),
                        spe_count,
                    });
                }
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

        let extensions = span.extensions();
        let tracer_span_data: &TracerSpanData = extensions
            .get()
            .expect("tracer span data to exist if span is closing");
        print_if_dbg(
            context,
            format!("Span {} closed. Sending to exporter", span_id.into_u64()),
        );
        self.send_subscriber_event_to_export(SubscriberEvent::ClosedSpan(ClosedSpan {
            trace_id: root_span_id.into_non_zero_u64().get(),
            duration: u64::try_from(
                tracer_span_data
                    .first_entered_at
                    .expect("first_entered_at to exist on span close")
                    .elapsed()
                    .as_nanos(),
            )
            .expect("span duration in nanos to fit u64"),
            span_id: span_id.into_u64(),
        }));
    }
}
