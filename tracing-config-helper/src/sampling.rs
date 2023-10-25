use api_structs::exporter::{SamplerLimits, TraceApplicationStats, TracerStats};
use std::collections::HashMap;
use std::time::Instant;

/// Whenever it returns true, it assumes the new trace, span or event was recorded
pub trait Sampler {
    fn allow_new_trace(&mut self, trace_name: &'static str) -> bool;
    fn allow_new_event(&mut self, trace_name: &'static str) -> bool;
    fn allow_new_orphan_event(&mut self) -> bool;
    fn allow_new_span(&mut self, trace_name: &'static str) -> bool;
    /// soe = span or event
    fn register_soe_dropped_on_export(&mut self);
    fn get_tracer_stats(&self) -> TracerStats;
}

impl Sampler for TracerSampler {
    fn allow_new_trace(&mut self, trace: &'static str) -> bool {
        if self.is_over_usage_limit_for_new_trace(trace) {
            self.register_dropped_trace(trace);
            false
        } else {
            self.register_single_span_or_event(trace);
            true
        }
    }

    fn allow_new_event(&mut self, trace: &'static str) -> bool {
        if self.is_over_usage_limit_for_existing_trace(trace) {
            self.register_dropped_trace(trace);
            false
        } else {
            self.register_single_span_or_event(trace);
            true
        }
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

    fn allow_new_span(&mut self, trace: &'static str) -> bool {
        self.register_single_span_or_event(trace);
        true
    }

    fn register_soe_dropped_on_export(&mut self) {
        self.register_soe_dropped_on_export();
    }

    fn get_tracer_stats(&self) -> TracerStats {
        TracerStats {
            spe_dropped_on_export: self.spe_dropped_on_export,
            orphan_events_per_minute_usage: self.orphan_events_per_minute_usage,
            logs_per_minute_dropped: self.orphan_events_per_minute_dropped,
            sampler_limits: self.sampler_limits.clone(),
            per_minute_trace_stats: self
                .trace_stats
                .iter()
                .map(|(trace, stats)| (trace.to_string(), stats.clone()))
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TracerSampler {
    current_window_start: Instant,
    spe_dropped_on_export: u32,
    orphan_events_per_minute_usage: u32,
    orphan_events_per_minute_dropped: u32,
    // we never remove entries because spans should be static, they never get removed from the application
    trace_stats: HashMap<&'static str, TraceApplicationStats>,
    pub sampler_limits: SamplerLimits,
}

impl TracerSampler {
    pub(crate) fn new(sampler_limits: SamplerLimits) -> Self {
        Self {
            current_window_start: Instant::now(),
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
                .saturating_sub(self.sampler_limits.logs_per_minute_limit);
            for trace_stats in self.trace_stats.values_mut() {
                trace_stats.spe_usage_per_minute = trace_stats.spe_usage_per_minute.saturating_sub(
                    self.sampler_limits
                        .new_trace_span_plus_event_per_minute_per_trace_limit,
                );
                trace_stats.dropped_traces_per_minute = 0;
            }
        }
    }
    pub fn register_soe_dropped_on_export(&mut self) {
        self.spe_dropped_on_export += 1;
    }

    pub fn register_dropped_trace(&mut self, trace: &'static str) {
        let entry = self
            .trace_stats
            .entry(trace)
            .or_insert(TraceApplicationStats {
                spe_usage_per_minute: 0,
                dropped_traces_per_minute: 0,
            });
        entry.dropped_traces_per_minute += 1;
    }
    #[allow(clippy::wrong_self_convention)]
    pub fn is_over_orphan_events_usage_limit(&mut self) -> bool {
        self.window_reset_check();
        self.orphan_events_per_minute_usage >= self.sampler_limits.logs_per_minute_limit
    }
    #[allow(clippy::wrong_self_convention)]
    pub fn is_over_usage_limit_for_new_trace(&mut self, trace: &'static str) -> bool {
        self.window_reset_check();

        let trace_stats = self
            .trace_stats
            .entry(trace)
            .or_insert(TraceApplicationStats {
                spe_usage_per_minute: 0,
                dropped_traces_per_minute: 0,
            });
        return trace_stats.spe_usage_per_minute
            >= self
                .sampler_limits
                .new_trace_span_plus_event_per_minute_per_trace_limit;
    }
    #[allow(clippy::wrong_self_convention)]
    pub fn is_over_usage_limit_for_existing_trace(&mut self, trace: &'static str) -> bool {
        self.window_reset_check();

        let trace_stats = self
            .trace_stats
            .entry(trace)
            .or_insert(TraceApplicationStats {
                spe_usage_per_minute: 0,
                dropped_traces_per_minute: 0,
            });
        return trace_stats.spe_usage_per_minute
            >= self
                .sampler_limits
                .existing_trace_span_plus_event_per_minute_limit;
    }

    pub fn register_dropped_orphan_event(&mut self) {
        self.orphan_events_per_minute_dropped += 1;
    }
    pub fn register_single_span_or_event(&mut self, trace: &'static str) {
        let trace_stats = self
            .trace_stats
            .entry(trace)
            .or_insert(TraceApplicationStats {
                spe_usage_per_minute: 0,
                dropped_traces_per_minute: 0,
            });
        trace_stats.spe_usage_per_minute = trace_stats.spe_usage_per_minute.saturating_add(1);
    }
    pub fn register_orphan_event(&mut self) {
        self.orphan_events_per_minute_usage += 1;
    }
}
