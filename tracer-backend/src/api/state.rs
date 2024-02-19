use crate::api::handlers::instance::connect::ChangeFilterInternalRequest;
use api_structs::ui::service::{InstanceDataPoint, ProfileData};
use api_structs::ServiceId;
use chrono::NaiveDateTime;
use sqlx::PgPool;
use std::collections::{HashMap, VecDeque};

pub type Shared<T> = std::sync::Arc<parking_lot::RwLock<T>>;

#[derive(Clone)]
pub struct AppState {
    pub con: PgPool,
    pub services_runtime_stats: Shared<HashMap<ServiceId, ServiceRuntimeData>>,
}

#[derive(Debug, Clone)]
pub struct InstanceState {
    pub id: i64,
    /// info
    pub rust_log: String,
    pub profile_data: Option<ProfileData>,
    // time data
    pub time_data_points: VecDeque<InstanceDataPoint>,
    pub see_handle: tokio::sync::mpsc::Sender<ChangeFilterInternalRequest>,
}

#[derive(Debug, Clone)]
pub struct ServiceRuntimeData {
    pub last_time_checked_for_alerts: NaiveDateTime,
    pub instances: HashMap<i64, InstanceState>,
}
