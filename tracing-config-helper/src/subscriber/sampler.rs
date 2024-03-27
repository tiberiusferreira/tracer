use api_structs::instance::update::Sampling;

/// Whenever it returns true, it assumes the new trace, span or event was recorded
pub trait Sampler {
    fn allow_new_trace(&mut self, trace_name: &str) -> bool;
    fn allow_new_event(&mut self, trace_name: &str) -> bool;
    fn allow_new_orphan_event(&mut self) -> bool;
    fn allow_new_span_kv(&mut self, trace_name: &str) -> bool;
}

impl Sampler for TracerSampler {
    fn allow_new_trace(&mut self, trace: &str) -> bool {
        match self.current_trace_sampling.traces.get(trace) {
            None => true,
            Some(sampling) => sampling.allow_new_traces(),
        }
    }

    fn allow_new_event(&mut self, trace: &str) -> bool {
        match self.current_trace_sampling.traces.get(trace) {
            None => true,
            Some(sampling) => sampling.allow_existing_trace_new_data(),
        }
    }

    fn allow_new_orphan_event(&mut self) -> bool {
        self.current_trace_sampling.allow_new_orphan_events
    }

    fn allow_new_span_kv(&mut self, trace: &str) -> bool {
        match self.current_trace_sampling.traces.get(trace) {
            None => true,
            Some(sampling) => sampling.allow_existing_trace_new_data(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TracerSampler {
    pub current_trace_sampling: Sampling,
}

impl TracerSampler {
    pub(crate) fn new() -> Self {
        Self {
            current_trace_sampling: Sampling::new_allow_everything(),
        }
    }
}
