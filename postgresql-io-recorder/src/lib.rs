use std::collections::{HashSet};
use std::panic::Location;
use std::sync::{Arc, RwLock};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;
use tracing_config_helper::io_provider::{specialize_events_or_panic, EventRecordingPlayhead, IoEventRequest};
use tracing_config_helper::SpecializedIoEvent;
use tracked_error::error_chain_to_pretty_formatted;
pub use crate::parameters::Parameter;
mod query_optional;
mod query_multiple;
pub const RECORDER_NAME: &str = "Postgres";
mod parameters;
mod placeholder_replacements;


impl DatabaseIoRecorder {
    pub fn from_global_recording() -> Self {
        let io_events = tracing_config_helper::io_provider::get_io_provider_recording_events(RECORDER_NAME).expect("Gel events to exist if in recording");
        let io_events: Vec<SpecializedIoEvent<IoEvent>> = specialize_events_or_panic(io_events);
        DatabaseIoRecorder::Recorded(Arc::new(RwLock::new(EventRecordingPlayhead { events: io_events, used_events: HashSet::new() })))
    }

    pub fn from_live_connection(pool: PgPool) -> Self {
        DatabaseIoRecorder::Live(pool)
    }
}


fn record_io_response_as_query_result<T: Serialize>(
    recorded_io_req: IoEventRequest,
    raw_io_response: Result<T, Error>,
) {
    let raw_io_response_json = raw_io_response.map(|value| {
        serde_json::to_value(&value).expect("sqlx response should always be serializable")
    });
    let is_err = raw_io_response_json.is_err();
    let io_response = IoEvent::QueryResult(QueryResult(raw_io_response_json));
    let io_response_json = io_response.as_json();
    recorded_io_req.record_response(io_response_json, is_err);
}


fn sqlx_error_to_recorder_error(
    e: sqlx::Error,
    query: &str,
    params: &IndexMap<String, Parameter>,
) -> Error {
    let err_str = error_chain_to_pretty_formatted(&e);
    let err_str = format!("{err_str} with query:\n{query}\nand parameters\n{params:#?}");
    Error::Internal {
        msg: err_str,
        location: Location::caller().to_string(),
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, Error, PartialEq)]
pub enum Error {
    #[error("Internal {msg} at {location}")]
    Internal { msg: String, location: String },
    #[error("Serde {msg} at {location}")]
    Serde { msg: String, location: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum IoEvent {
    QueryRequest(QueryRequest),
    QueryResult(QueryResult),
    TxStartRequest,
    TxStartResult(TxStartResult),
    TxCommitRequest(TxCommitRequest),
    TxCommitResult(TxCommitResult),
}

impl IoEvent {
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("io req should always be serializable")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryWithParameters {
    pub query_text: String,
    pub query_type: QueryType,
    pub parameters: IndexMap<String, Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QueryType {
    Optional,
    RequiredSingle,
    Multiple,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QueryRequest {
    pub tx_id: Option<Uuid>,
    pub query_text: String,
    pub query_type: QueryType,
    pub parameters: IndexMap<String, Parameter>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QueryResult(Result<serde_json::Value, Error>);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TxCommitRequest {
    pub tx_id: Uuid,
}

pub type TxId = Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TxStartResult(Result<TxId, Error>);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TxCommitResult(Result<(), Error>);

#[derive(Clone)]
pub enum DatabaseIoRecorder {
    Recorded(Arc<RwLock<EventRecordingPlayhead<IoEvent>>>),
    Live(sqlx::PgPool),
}

pub enum TransactionIoProvider<'a> {
    Recorded(Arc<RwLock<EventRecordingPlayhead<IoEvent>>>),
    Live(sqlx::Transaction<'a, sqlx::Postgres>),
}

