use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use indexmap::IndexMap;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstanceSnapshot {
    pub instance_id: Uuid,
    pub execution_recordings: Vec<ExecutionRecordingSnapshot>,
    pub cpu_profile_base64: Option<String>,
}

pub type IoRecorderName = String;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IoEvent {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub is_response_of: Option<Uuid>,
    pub is_error: bool,
    pub value: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionIOFragment {
    pub input: Option<serde_json::Value>,
    pub io_providers_events: HashMap<IoRecorderName, Vec<IoEvent>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionRecordingSnapshot {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ended: bool,
    pub execution_io_fragment: ExecutionIOFragment,
    pub attributes: IndexMap<String, HashSet<String>>,
}

