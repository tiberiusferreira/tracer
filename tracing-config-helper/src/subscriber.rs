use crate::sampling::{Sampler, TracerSampler};
use crate::{print_if_dbg, TracerTracingSubscriber};
use api_structs::exporter::{
    ClosedSpan, NewOrphanEvent, NewSpan, NewSpanEvent, SamplerLimits, Severity, SubscriberEvent,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::field::{Field, Visit};
use tracing::span::Record;
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

    fn span_root<'a, S: Subscriber + for<'b> LookupSpan<'b>>(
        id: Id,
        ctx: &'a Context<S>,
    ) -> Option<SpanRef<'a, S>> {
        let root = ctx.span(&id)?.scope().from_root().next()?;
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
                self.sampler.write().register_soe_dropped_on_export();
            }
        }
    }
}

struct MyV {
    message: Option<String>,
}
impl Visit for MyV {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        }
    }
}

struct TracerSpanData {
    first_entered_at: std::time::Instant,
}
struct TracerRootSpanData {
    dropped: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanEvent {
    pub name: String,
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
        let event_data = {
            let mut my_v = MyV { message: None };
            event.record(&mut my_v);
            let name = if let Some(msg) = my_v.message {
                print_if_dbg(context, format!("New event: {msg}"));
                msg
            } else {
                print_if_dbg(context, format!("New empty event, dropping it"));
                println!("ALERT: Empty events are not supported yet!");
                return;
            };
            let level = match event.metadata().level() {
                &tracing::metadata::Level::TRACE => Severity::Trace,
                &tracing::metadata::Level::DEBUG => Severity::Debug,
                &tracing::metadata::Level::INFO => Severity::Info,
                &tracing::metadata::Level::WARN => Severity::Warn,
                &tracing::metadata::Level::ERROR => Severity::Error,
            };
            SpanEvent {
                name,
                timestamp: api_structs::time_conversion::now_nanos_u64(),
                level,
                key_vals: Default::default(),
            }
        };

        let Some(span) = span else {
            print_if_dbg(context, format!("Event is orphan"));
            let mut w_sampler = self.sampler.write();
            let new_orphan_event_allowed = w_sampler.allow_new_orphan_event();
            drop(w_sampler);
            return if new_orphan_event_allowed {
                print_if_dbg(context, format!("Allowed by sampler, sending to exporter"));
                self.send_subscriber_event_to_export(SubscriberEvent::NewOrphanEvent(
                    NewOrphanEvent {
                        name: event_data.name,
                        timestamp: event_data.timestamp,
                        level: event_data.level,
                        key_vals: event_data.key_vals,
                    },
                ));
            } else {
                print_if_dbg(context, format!("Not Allowed by sampler, dropping"));
            };
        };
        if Self::trace_was_dropped(span.id(), &ctx) {
            print_if_dbg(
                context,
                format!("Event belongs to trace previously dropped, dropping event."),
            );
            return;
        }
        let mut w_sampler = self.sampler.write();
        let root = Self::span_root(span.id(), &ctx).expect("root span to exist");
        let new_event_allowed = w_sampler.allow_new_event(root.name());
        drop(w_sampler);
        if new_event_allowed {
            print_if_dbg(context, format!("Allowed by sampler, sending to exporter."));
            self.send_subscriber_event_to_export(SubscriberEvent::NewSpanEvent(NewSpanEvent {
                trace_id: root.id().into_non_zero_u64(),
                span_id: span.id().into_non_zero_u64(),
                name: event_data.name,
                timestamp: event_data.timestamp,
                level: event_data.level,
                key_vals: event_data.key_vals,
            }));
        }
    }
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let context = "on_enter";
        let span = ctx.span(id).expect("entered span to exist!");
        let span_name = span.name();
        let mut extensions = span.extensions_mut();
        let tracer_span_data: Option<&mut TracerSpanData> = extensions.get_mut();
        if tracer_span_data.is_some() {
            print_if_dbg(context, format!("span {span_name} entered again"));
            return;
        } else {
            print_if_dbg(
                context,
                format!("span {span_name} entered for the first time"),
            );
            extensions.insert(TracerSpanData {
                first_entered_at: std::time::Instant::now(),
            });
        }
        drop(extensions);
        let root_span = Self::span_root(id.clone(), &ctx).expect("root span to exist");

        if root_span.id() == *id {
            print_if_dbg(context, format!("Span is root. Id: {}", id.into_u64()));
            let mut w_sampler = self.sampler.write();
            let new_trace_allowed = w_sampler.allow_new_trace(&root_span.name());
            drop(w_sampler);
            if new_trace_allowed {
                print_if_dbg(context, "Allowed by sampler, sending to exporter");
                self.send_subscriber_event_to_export(SubscriberEvent::NewSpan(NewSpan {
                    id: id.into_non_zero_u64(),
                    trace_id: root_span.id().into_non_zero_u64(),
                    name: span_name.to_string(),
                    parent_id: None,
                    timestamp: u64::try_from(
                        chrono::Utc::now()
                            .naive_utc()
                            .timestamp_nanos_opt()
                            .unwrap(),
                    )
                    .unwrap(),
                    key_vals: Default::default(),
                }));
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
                let mut w_sampler = self.sampler.write();
                let new_span_allowed = w_sampler.allow_new_span(root_span.name());
                drop(w_sampler);
                if new_span_allowed {
                    let parent_id = span.parent().expect("parent to exist if non-root").id();
                    print_if_dbg(context, "Allowed by sampler, sending to exporter");
                    self.send_subscriber_event_to_export(SubscriberEvent::NewSpan(NewSpan {
                        id: id.into_non_zero_u64(),
                        trace_id: root_span.id().into_non_zero_u64(),
                        name: span_name.to_string(),
                        parent_id: Some(parent_id.into_non_zero_u64()),
                        timestamp: u64::try_from(
                            chrono::Utc::now()
                                .naive_utc()
                                .timestamp_nanos_opt()
                                .unwrap(),
                        )
                        .unwrap(),
                        key_vals: Default::default(),
                    }));
                } else {
                    print_if_dbg(context, "Not Allowed by sampler");
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
        let extensions = span.extensions();
        let tracer_span_data: &TracerSpanData = extensions
            .get()
            .expect("tracer span data to exist if span is closing");
        print_if_dbg(
            context,
            format!("Span {} closed. Sending to exporter", span_id.into_u64()),
        );
        self.send_subscriber_event_to_export(SubscriberEvent::ClosedSpan(ClosedSpan {
            id: span_id.into_non_zero_u64(),
            duration: u64::try_from(tracer_span_data.first_entered_at.elapsed().as_nanos())
                .expect("span duration in nanos to fit u64"),
        }));
    }
}
