use api_structs::exporter::{SamplerStatus, TraceSummary};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Whenever it returns true, it assumes the new trace, span or event was recorded
pub trait Sampler {
    fn allow_new_trace(
        &mut self,
        name: &str,
        existing_traces: &[TraceSummary],
        orphan_events_len: usize,
    ) -> bool;
    fn allow_new_event(
        &mut self,
        name: &str,
        existing_traces: &[TraceSummary],
        orphan_events_len: usize,
    ) -> bool;
    fn allow_new_orphan_event(
        &mut self,
        existing_traces: &[TraceSummary],
        orphan_events_len: usize,
    ) -> bool;
    fn allow_new_span(
        &mut self,
        name: &str,
        existing_traces: &[TraceSummary],
        orphan_events_len: usize,
    ) -> bool;
    fn get_sampler_status(&self) -> SamplerStatus;
    fn reset_hard_limit_hit(&mut self);
}

/// Doesn't allow any new data to be recorded after hard_se_storage_limit is hit
/// Keeps a budget of spans+events per minute per trace. Allows existing traces to keep recording even after their
/// budget limit is exceeded, making their budget negative.
/// Overtime the budget recovers at per_trace_se_per_minute_limit rate.
///
pub struct TracerSampler {
    hard_se_storage_limit: usize,
    quota_keeper: QuotaKeeper,
    hard_limit_hit: bool,
}

impl TracerSampler {
    pub fn new(hard_se_storage_limit: usize, se_per_minute_limit: i64) -> TracerSampler {
        Self {
            hard_se_storage_limit,
            quota_keeper: QuotaKeeper::new(Duration::from_secs(60), se_per_minute_limit),
            hard_limit_hit: false,
        }
    }
    fn hard_limit_hit(&self, existing_traces: &[TraceSummary], orphan_events_len: usize) -> bool {
        let se_total = existing_traces.iter().fold(0usize, |acc, curr| {
            let se = curr.events + curr.spans;
            se + acc
        }) + orphan_events_len;

        se_total >= self.hard_se_storage_limit
    }
}

impl Sampler for TracerSampler {
    fn allow_new_trace(
        &mut self,
        trace: &str,
        existing_traces: &[TraceSummary],
        orphan_events_len: usize,
    ) -> bool {
        if self.hard_limit_hit(existing_traces, orphan_events_len) {
            self.hard_limit_hit = true;
            return false;
        } else if self.quota_keeper.get_remaining_quota(trace) > 0 {
            self.quota_keeper.decrease_quota(trace);
            true
        } else {
            false
        }
    }

    fn allow_new_event(
        &mut self,
        trace: &str,
        existing_traces: &[TraceSummary],
        orphan_events_len: usize,
    ) -> bool {
        return if self.hard_limit_hit(existing_traces, orphan_events_len) {
            self.hard_limit_hit = true;
            false
        } else {
            self.quota_keeper.decrease_quota(trace);
            true
        };
    }

    fn allow_new_orphan_event(
        &mut self,
        existing_traces: &[TraceSummary],
        orphan_events_len: usize,
    ) -> bool {
        return if self.hard_limit_hit(existing_traces, orphan_events_len) {
            self.hard_limit_hit = true;
            false
        } else {
            true
        };
    }

    fn allow_new_span(
        &mut self,
        trace: &str,
        existing_traces: &[TraceSummary],
        orphan_events_len: usize,
    ) -> bool {
        return if self.hard_limit_hit(existing_traces, orphan_events_len) {
            self.hard_limit_hit = true;
            false
        } else {
            self.quota_keeper.decrease_quota(trace);
            true
        };
    }

    fn get_sampler_status(&self) -> SamplerStatus {
        SamplerStatus {
            hard_se_storage_limit: self.hard_se_storage_limit,
            hard_limit_hit: self.hard_limit_hit,
            window_duration: u64::try_from(self.quota_keeper.window_duration.as_nanos())
                .expect("duration to fit u64"),
            trace_se_quota_per_window: self.quota_keeper.trace_se_quota_per_window,
        }
    }

    fn reset_hard_limit_hit(&mut self) {
        self.hard_limit_hit = false;
    }
}

struct QuotaKeeper {
    current_window_start: Instant,
    window_duration: Duration,
    trace_se_quota_per_window: i64,
    // we never remove entries because spans should be static, they never get removed from the application
    remaining_se_quota_per_trace: HashMap<String, i64>,
}

impl QuotaKeeper {
    fn new(window_duration: Duration, trace_se_quota_per_window: i64) -> Self {
        Self {
            current_window_start: Instant::now(),
            window_duration,
            trace_se_quota_per_window,
            remaining_se_quota_per_trace: HashMap::new(),
        }
    }
    fn window_reset_check(&mut self) {
        let current_window_start = self.current_window_start;
        if current_window_start.elapsed() > self.window_duration {
            self.current_window_start = Instant::now();
            for quota_remaining in self.remaining_se_quota_per_trace.values_mut() {
                // add quota
                (*quota_remaining) += self.trace_se_quota_per_window;
                if (*quota_remaining) > self.trace_se_quota_per_window {
                    // clamp it
                    (*quota_remaining) = self.trace_se_quota_per_window;
                }
            }
        }
    }
    pub fn get_remaining_quota(&mut self, trace: &str) -> i64 {
        self.window_reset_check();
        let se_quota_per_trace = self
            .remaining_se_quota_per_trace
            .entry(trace.to_string())
            .or_insert(self.trace_se_quota_per_window);
        return *se_quota_per_trace;
    }
    /// Returns false if ran out of quota
    pub fn decrease_quota(&mut self, trace: &str) {
        let se_quota_per_trace = self
            .remaining_se_quota_per_trace
            .entry(trace.to_string())
            .or_insert(self.trace_se_quota_per_window);
        *se_quota_per_trace = se_quota_per_trace.saturating_sub(1);
    }
}
