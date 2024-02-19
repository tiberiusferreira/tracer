use crate::api::handlers::instance::connect::ChangeFilterInternalRequest;
use crate::CONSIDER_DEAD_INSTANCE_AFTER_NO_DATA_FOR_SECONDS;
use api_structs::time_conversion::{nanos_to_secs, now_nanos_u64};
use api_structs::ui::service::{InstanceDataPoint, ProfileData};
use api_structs::ServiceId;
use chrono::NaiveDateTime;
use sqlx::PgPool;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use tracing::{debug, info, trace};

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
    /// info
    pub rust_log: String,
    pub profile_data: Option<ProfileData>,
    // time data
    pub time_data_points: VecDeque<InstanceDataPoint>,
    pub see_handle: tokio::sync::mpsc::Sender<ChangeFilterInternalRequest>,
}

impl InstanceState {
    pub fn seconds_since_last_seen(&self) -> u64 {
        match self.time_data_points.back() {
            None => self.created_at.elapsed().as_secs(),
            Some(latest_data_point) => {
                nanos_to_secs(now_nanos_u64().saturating_sub(latest_data_point.timestamp))
            }
        }
    }
    pub fn trim_data_points_to(&mut self, max_points: usize) {
        while self.time_data_points.len() > max_points {
            self.time_data_points.pop_front();
        }
    }
    pub fn is_dead(&self) -> bool {
        match self.time_data_points.is_empty() {
            true => {
                // maybe the instance was just created, so we wait a bit if it has no data
                let age_seconds = self.created_at.elapsed().as_secs();
                trace!(
                    "Instance {} has no data and is {}s old",
                    self.id,
                    age_seconds
                );
                if age_seconds > (CONSIDER_DEAD_INSTANCE_AFTER_NO_DATA_FOR_SECONDS as u64) {
                    trace!("Considering it dead");
                    true
                } else {
                    trace!("Considering it alive");
                    false
                }
            }
            false => {
                let seconds_last_seen = self.seconds_since_last_seen();
                debug!("Instance {} last seen {}s ago", self.id, seconds_last_seen);
                if (CONSIDER_DEAD_INSTANCE_AFTER_NO_DATA_FOR_SECONDS as u64) < seconds_last_seen {
                    info!("Instance is dead");
                    true
                } else {
                    info!("Instance is alive");
                    false
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceRuntimeData {
    pub last_time_checked_for_alerts: NaiveDateTime,
    pub instances: HashMap<i64, InstanceState>,
}
