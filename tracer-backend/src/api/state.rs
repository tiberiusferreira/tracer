use api_structs::TraceName;
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::Instant;

pub type Shared<T> = std::sync::Arc<parking_lot::RwLock<T>>;

#[derive(Clone)]
pub struct AppState {
    pub con: PgPool,
}

#[derive(Debug, Clone)]
pub struct BytesBudgetUsage {
    pub current_window_start: Instant,
    pub window_size_sec: u32,
    pub budget_per_window_bytes: u32,
    pub traces_usage_bytes: HashMap<TraceName, u32>,
    pub orphan_events_usage: u32,
}

impl BytesBudgetUsage {
    pub fn new(window_size_sec: u32, increase_amount_per_window_bytes: u32) -> Self {
        BytesBudgetUsage {
            current_window_start: Instant::now(),
            window_size_sec,
            budget_per_window_bytes: increase_amount_per_window_bytes,
            traces_usage_bytes: HashMap::new(),
            orphan_events_usage: 0,
        }
    }
    pub fn update(&mut self) {
        if (self.window_size_sec as u64) < self.current_window_start.elapsed().as_secs() {
            self.current_window_start = Instant::now();
            self.orphan_events_usage = self
                .orphan_events_usage
                .saturating_sub(self.budget_per_window_bytes);
            for v in self.traces_usage_bytes.values_mut() {
                *v = v.saturating_sub(self.budget_per_window_bytes);
            }
        }
    }
    pub fn increase_orphan_events_usage_by(&mut self, amount: u32) {
        self.orphan_events_usage += amount;
    }
    pub fn increase_trace_usage_by(&mut self, trace_name: &str, amount: u32) {
        let usage = self
            .traces_usage_bytes
            .entry(trace_name.to_string())
            .or_insert(0);
        *usage += amount;
    }
    pub fn is_trace_over_budget(&self, trace_name: &str) -> bool {
        let Some(usage) = self.traces_usage_bytes.get(trace_name) else {
            return false;
        };
        return self.budget_per_window_bytes < *usage;
    }
    pub fn is_orphan_events_over_budget(&self) -> bool {
        self.budget_per_window_bytes < self.orphan_events_usage
    }
}
