pub use crate::Severity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstanceSnapshot {
    /// This id should be incremented sequentially after a given update is successfully sent.
    /// The client is required to retry sending the update until it receives an OK response back, but it could
    /// accidentally send it twice, due to a timeout that would eventually be an OK.
    /// This id helps the Collector discard duplicate updates
    pub update_count: u64,
    pub instance_id: uuid::Uuid,
    pub execution_recordings: Vec<ExecutionRecording>,
    pub export_buffer_size_bytes: u64,
    pub cpu_profile_base64: Option<String>,
}

pub type IoRecorderName = String;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayData {
    pub input: serde_json::Value,
    pub io_providers_events: HashMap<IoRecorderName, Vec<serde_json::Value>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionRecording {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ended: bool,
    // metrics, something we need to group by keys
    // examples: users, group by id, but then we have envs, so we also group by envs
    // keys might have one or more values.
    // order_id => ["2", "5"]
    // user_id => ["32", "78"]
    // status_code => ["200"]
    // retry_count => ["1", "2", "3"]
    pub replay_data: ReplayData,
    pub executed_functions: Vec<ExecutingFunction>,
    pub attributes: HashMap<String, HashSet<String>>,
    // support data
    pub current_call_stack: Vec<u64>,
    pub function_count: u64,
    pub recording_enabled: bool,
}

impl ExecutionRecording {
    pub fn new(id: Uuid, input: serde_json::Value, recording_enabled: bool) -> ExecutionRecording {
        Self {
            id,
            function_count: 0,
            replay_data: ReplayData {
                input,
                io_providers_events: Default::default(),
            },
            started_at: Utc::now(),
            last_seen_at: Utc::now(),
            ended: false,
            executed_functions: vec![],
            current_call_stack: vec![],
            attributes: Default::default(),
            recording_enabled,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutingFunction {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub module: String,
    pub filename: String,
    pub line: u32,
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Location {
    pub module: Option<String>,
    pub filename: Option<String>,
    pub line: Option<u32>,
}
