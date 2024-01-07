use crate::api::ChangeFilterInternalRequest;
use api_structs::ui::service_health::{AlertConfig, InstanceDataPoint, ProfileData};
use api_structs::Env;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{HashMap, VecDeque};

pub type Shared<T> = std::sync::Arc<parking_lot::RwLock<T>>;

#[derive(Clone)]
pub struct AppState {
    pub con: PgPool,
    pub instance_runtime_stats: Shared<HashMap<ServiceId, ServiceData>>,
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
pub struct ServiceData {
    pub alert_config: AlertConfig,
    pub instances: HashMap<i64, InstanceState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct ServiceId {
    /// tracer-backend
    pub name: String,
    /// Local
    pub env: Env,
}
