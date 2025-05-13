pub use crate::Severity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use uuid::Uuid;

pub struct InstanceUpdateEndpoint;
impl crate::Endpoint for InstanceUpdateEndpoint {
    const PATH: &'static str = "/api/instance/update";
    const METHOD: &'static str = "POST";
    type RequestBody = InstanceSnapshot;
    type QueryParameters = ();
    type ResponseBody = ConfigChange;
}

pub const ROOT_SPAN_COUNT_ID: u64 = 1;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfigChange {
    pub log_filter: Option<String>,
}

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayData {
    pub input: serde_json::Value,
    pub database_recording: DatabaseRecording,
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
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
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
                database_recording: DatabaseRecording::default(),
            },
            started_at: Utc::now(),
            last_seen_at: Utc::now(),
            ended: false,
            warnings: Default::default(),
            errors: Default::default(),
            executed_functions: vec![],
            current_call_stack: vec![],
            attributes: Default::default(),
            recording_enabled,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryWithResult {
    pub id: u64,
    pub started_at: DateTime<Utc>,
    pub query_with_parameters: QueryWithParameters,
    pub result: Option<QueryResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryResult {
    pub ended_at: DateTime<Utc>,
    pub result: Result<serde_json::Value, Error>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub id: u64,
    pub started_at: DateTime<Utc>,
    pub queries_count: u64,
    pub queries: Vec<QueryWithResult>,
    pub result: Option<TransactionResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionResult {
    pub ended_at: DateTime<Utc>,
    pub result: Result<(), Error>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DatabaseRecording {
    pub standalone_queries_count: u64,
    pub standalone_queries: Vec<QueryWithResult>,
    pub transactions_count: u64,
    pub transactions: Vec<Transaction>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
pub enum Error {
    #[error("Internal {msg} at {location}")]
    Internal { msg: String, location: String },
    #[error("Serde {msg} at {location}")]
    Serde { msg: String, location: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryWithParameters {
    pub query_text: String,
    pub parameters: HashMap<String, Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Parameter {
    Uuid {
        val: Uuid,
        cast_to_table: Option<String>,
    },
    String(String),
    Datetime(DateTime<Utc>),
    Bool(bool),
    Json(serde_json::Value),
    I32(i32),
}
