use crate::api::ChangeFilterInternalRequest;
use api_structs::ui::service_health::{AlertConfig, InstanceDataPoint};
use api_structs::Env;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;

pub type Shared<T> = std::sync::Arc<parking_lot::RwLock<T>>;

#[derive(Clone)]
pub struct AppState {
    pub con: PgPool,
    pub instance_runtime_stats: Shared<HashMap<ServiceId, ServiceData>>,
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub id: i64,
    /// info
    pub rust_log: String,
    // time data
    pub time_data_points: Vec<InstanceDataPoint>,
    pub see_handle: Option<tokio::sync::mpsc::Sender<ChangeFilterInternalRequest>>,
}

#[derive(Debug, Clone)]
pub struct ServiceData {
    pub config: ServiceConfig,
    pub instances: HashMap<i64, Instance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct ServiceId {
    /// tracer-backend
    pub name: String,
    /// Local
    pub env: Env,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceConfig {
    pub alert_config: AlertConfig,
}
