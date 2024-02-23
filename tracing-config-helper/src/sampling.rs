use api_structs::instance::update::Sampling;
use rand::random;

/// Whenever it returns true, it assumes the new trace, span or event was recorded
pub trait Sampler {
    fn allow_new_trace(&mut self, trace_name: &str) -> bool;
    fn allow_new_event(&mut self, trace_name: &str) -> bool;
    fn allow_new_orphan_event(&mut self) -> bool;
    fn allow_new_span(&mut self, trace_name: &str) -> bool;
}

impl Sampler for TracerSampler {
    fn allow_new_trace(&mut self, trace: &str) -> bool {
        match self.current_trace_sampling.traces.get(trace) {
            None => true,
            Some(sampling) => {
                let random_number: f32 = random();
                random_number <= sampling.new_traces_sampling_rate_0_to_1
            }
        }
    }

    fn allow_new_event(&mut self, trace: &str) -> bool {
        match self.current_trace_sampling.traces.get(trace) {
            None => true,
            Some(sampling) => {
                let random_number: f32 = random();
                random_number <= sampling.existing_traces_new_data_sampling_rate_0_to_1
            }
        }
    }

    fn allow_new_orphan_event(&mut self) -> bool {
        let random_number: f32 = random();
        random_number
            <= self
                .current_trace_sampling
                .orphan_events_sampling_rate_0_to_1
    }

    fn allow_new_span(&mut self, trace: &str) -> bool {
        match self.current_trace_sampling.traces.get(trace) {
            None => true,
            Some(sampling) => {
                let random_number: f32 = random();
                random_number <= sampling.existing_traces_new_data_sampling_rate_0_to_1
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct TracerSampler {
    pub current_trace_sampling: Sampling,
    pub export_buffer_capacity: u64,
    pub export_buffer_usage: u64,
}

impl TracerSampler {
    pub(crate) fn new() -> Self {
        Self {
            current_trace_sampling: Sampling::new_allow_everything(),
            export_buffer_capacity: 0,
            export_buffer_usage: 0,
        }
    }
}
