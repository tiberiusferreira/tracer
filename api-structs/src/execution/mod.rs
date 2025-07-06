use crate::instance::update::ReplayDataFragment;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    pub external_id: Uuid,
    pub size_bytes: u64,
    pub service_instance_id: Uuid,
    pub service_env: String,
    pub service_name: String,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ended: bool,
    pub replay_data: ReplayDataFragment,
    pub attributes: Vec<Attribute>,
}
