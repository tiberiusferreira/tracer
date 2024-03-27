use crate::api::handlers::instance::connect::ChangeFilterInternalRequest;
use crate::CONSIDER_DEAD_INSTANCE_AFTER_NO_DATA_FOR_SECONDS;
use api_structs::time_conversion::time_from_nanos;
use api_structs::ui::service::{OrphanEvent, ProfileData, TraceHeader};
use api_structs::{ServiceId, TraceName};
use chrono::NaiveDateTime;
use sqlx::PgPool;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use tracing::debug;

pub type Shared<T> = std::sync::Arc<parking_lot::RwLock<T>>;

#[derive(Clone)]
pub struct AppState {
    pub con: PgPool,
    pub services_runtime_stats: Shared<HashMap<ServiceId, ServiceRuntimeData>>,
}

#[derive(Debug, Clone)]
pub struct InstanceState {
    pub id: i64,
    pub created_at: Instant,
    pub last_seen: Instant,
    /// info
    pub rust_log: String,
    pub profile_data: Option<ProfileData>,
    // time data
    pub see_handle: tokio::sync::mpsc::Sender<ChangeFilterInternalRequest>,
}

impl InstanceState {
    pub fn seconds_since_last_seen(&self) -> u64 {
        self.last_seen.elapsed().as_secs()
    }
    pub fn is_dead(&self) -> bool {
        let seconds_last_seen = self.seconds_since_last_seen();
        debug!("Instance {} last seen {}s ago", self.id, seconds_last_seen);
        if (CONSIDER_DEAD_INSTANCE_AFTER_NO_DATA_FOR_SECONDS as u64) < seconds_last_seen {
            true
        } else {
            false
        }
    }
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
        return self.budget_per_window_bytes < self.orphan_events_usage;
    }
}

#[derive(Debug, Clone)]
pub struct ServiceDataPoint {
    pub timestamp: u64,
    pub instance_id: i64,
    pub traces: Vec<TraceHeader>,
    pub orphan_events: Vec<OrphanEvent>,
    pub budget_usage: BytesBudgetUsage,
}

#[derive(Debug, Clone)]
pub struct ServiceRuntimeData {
    pub last_time_checked_for_alerts: NaiveDateTime,
    pub service_data_points: VecDeque<ServiceDataPoint>,
    pub instances: HashMap<i64, InstanceState>,
}

impl ServiceRuntimeData {
    pub fn data_points_since_last_alert_check_reversed(
        &self,
    ) -> impl Iterator<Item = &ServiceDataPoint> {
        self.service_data_points
            .iter()
            .rev()
            .take_while(|e| self.last_time_checked_for_alerts <= time_from_nanos(e.timestamp))
    }
}
