use api_structs::exporter::{SamplerLimits, TraceApplicationStats, TracerStats};
use std::collections::HashMap;
use std::time::Instant;

/// Whenever it returns true, it assumes the new trace, span or event was recorded
pub trait Sampler {
    fn allow_new_trace(&mut self, name: &str) -> bool;
    fn allow_new_event(&mut self, name: &str) -> bool;
    fn allow_new_orphan_event(&mut self) -> bool;
    fn allow_new_span(&mut self, name: &str) -> bool;
    /// soe = span or event
    fn register_soe_dropped_on_export(&mut self);
    fn register_reconnect(&mut self);
    fn get_tracer_stats(&mut self) -> TracerStats;
}

/// Doesn't allow any new data to be recorded after hard_se_storage_limit is hit
/// Keeps a budget of spans+events per minute per trace. Allows existing traces to keep recording even after their
/// budget limit is exceeded, making their budget negative.
/// Overtime the budget recovers at per_trace_se_per_minute_limit rate.
///

impl Sampler for TracerSampler {
    fn allow_new_trace(&mut self, trace: &str) -> bool {
        if self.is_over_usage_limit(trace) {
            self.register_dropped_trace(trace);
            false
        } else {
            self.register_single_span_or_event(trace);
            true
        }
    }

    fn allow_new_event(&mut self, trace: &str) -> bool {
        self.register_single_span_or_event(trace);
        true
    }

    fn allow_new_orphan_event(&mut self) -> bool {
        if self.is_over_orphan_events_usage_limit() {
            self.register_dropped_orphan_event();
            false
        } else {
            self.register_orphan_event();
            true
        }
    }

    fn allow_new_span(&mut self, trace: &str) -> bool {
        self.register_single_span_or_event(trace);
        true
    }

    fn register_soe_dropped_on_export(&mut self) {
        self.register_soe_dropped_on_export();
    }
    fn register_reconnect(&mut self) {
        self.register_reconnect();
    }

    fn get_tracer_stats(&mut self) -> TracerStats {
        TracerStats {
            reconnects: self.reconnects,
            spe_dropped_on_export: self.spe_dropped_on_export,
            orphan_events_per_minute_usage: self.orphan_events_per_minute_usage,
            orphan_events_per_minute_dropped: self.orphan_events_per_minute_dropped,
            per_minute_trace_stats: self.trace_stats.clone(),
        }
    }
}

pub struct TracerSampler {
    current_window_start: Instant,
    reconnects: u32,
    spe_dropped_on_export: u32,
    orphan_events_per_minute_usage: u32,
    orphan_events_per_minute_dropped: u32,
    // we never remove entries because spans should be static, they never get removed from the application
    trace_stats: HashMap<String, TraceApplicationStats>,
    pub sampler_limits: SamplerLimits,
}

impl TracerSampler {
    pub(crate) fn new(sampler_limits: SamplerLimits) -> Self {
        Self {
            current_window_start: Instant::now(),
            reconnects: 0,
            sampler_limits,
            spe_dropped_on_export: 0,
            orphan_events_per_minute_usage: 0,
            orphan_events_per_minute_dropped: 0,
            trace_stats: HashMap::new(),
        }
    }
    fn window_reset_check(&mut self) {
        let current_window_start = self.current_window_start;

        if current_window_start.elapsed().as_secs() >= 60 {
            self.current_window_start = Instant::now();
            self.orphan_events_per_minute_dropped = 0;
            self.orphan_events_per_minute_usage = self
                .orphan_events_per_minute_usage
                .saturating_sub(self.sampler_limits.orphan_events_per_minute_limit);
            for trace_stats in self.trace_stats.values_mut() {
                trace_stats.spe_usage_per_minute = trace_stats.spe_usage_per_minute.saturating_sub(
                    self.sampler_limits
                        .span_plus_event_per_minute_per_trace_limit,
                );
                trace_stats.dropped_per_minute = 0;
            }
        }
    }
    pub fn register_soe_dropped_on_export(&mut self) {
        self.spe_dropped_on_export += 1;
    }
    pub fn register_reconnect(&mut self) {
        self.reconnects += 1;
    }
    pub fn register_dropped_trace(&mut self, trace: &str) {
        let entry = self
            .trace_stats
            .entry(trace.to_string())
            .or_insert(TraceApplicationStats {
                spe_usage_per_minute: 0,
                dropped_per_minute: 0,
            });
        entry.dropped_per_minute += 1;
    }
    pub fn is_over_orphan_events_usage_limit(&mut self) -> bool {
        self.window_reset_check();
        self.orphan_events_per_minute_usage >= self.sampler_limits.orphan_events_per_minute_limit
    }
    pub fn is_over_usage_limit(&mut self, trace: &str) -> bool {
        self.window_reset_check();

        let trace_stats =
            self.trace_stats
                .entry(trace.to_string())
                .or_insert(TraceApplicationStats {
                    spe_usage_per_minute: 0,
                    dropped_per_minute: 0,
                });
        return trace_stats.spe_usage_per_minute
            >= self
                .sampler_limits
                .span_plus_event_per_minute_per_trace_limit;
    }

    pub fn register_dropped_orphan_event(&mut self) {
        self.orphan_events_per_minute_dropped += 1;
    }
    pub fn register_single_span_or_event(&mut self, trace: &str) {
        let trace_stats =
            self.trace_stats
                .entry(trace.to_string())
                .or_insert(TraceApplicationStats {
                    spe_usage_per_minute: 0,
                    dropped_per_minute: 0,
                });
        trace_stats.spe_usage_per_minute = trace_stats.spe_usage_per_minute.saturating_add(1);
    }
    pub fn register_orphan_event(&mut self) {
        self.orphan_events_per_minute_usage += 1;
    }
}
