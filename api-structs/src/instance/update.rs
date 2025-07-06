use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstanceSnapshot {
    pub instance_id: Uuid,
    pub execution_recordings: Vec<ExecutionRecording>,
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
pub struct ReplayDataFragment {
    pub input: Option<serde_json::Value>,
    pub io_providers_events: HashMap<IoRecorderName, Vec<IoEvent>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionRecording {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ended: bool,
    pub replay_data_fragment: ReplayDataFragment,
    pub attributes: HashMap<String, HashSet<String>>,
    pub recording_enabled: bool,
}

impl ExecutionRecording {
    pub fn new(id: Uuid, input: serde_json::Value, recording_enabled: bool) -> ExecutionRecording {
        Self {
            id,
            replay_data_fragment: ReplayDataFragment {
                input: Some(input),
                io_providers_events: Default::default(),
            },
            started_at: Utc::now(),
            last_seen_at: Utc::now(),
            ended: false,
            attributes: Default::default(),
            recording_enabled,
        }
    }
}
