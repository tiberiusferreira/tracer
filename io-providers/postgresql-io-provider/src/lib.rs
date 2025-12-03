pub use crate::parameters::Parameter;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres};
use std::collections::HashSet;
use std::panic::Location;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracer::player_api::{EventRecordingPlayhead, SpecializedIoEvent, specialize_events_or_panic};
use tracer::recorder_api::{IoEventRequest, record_io_event_request_or_panic};
use tracked_error::error_chain_to_pretty_formatted;
use uuid::Uuid;
mod query_multiple;
mod query_optional;
mod tx_start;
mod tx_commit;
mod execute;
pub const RECORDER_NAME: &str = "Postgres";
mod parameters;

impl PgIoRecorder {
    pub fn from_global_recording() -> Self {
        let io_events = tracer::player_api::get_io_provider_recorded_events(RECORDER_NAME)
            .expect("Gel events to exist if in recording");
        let io_events: Vec<SpecializedIoEvent<IoEvent>> = specialize_events_or_panic(io_events);
        PgIoRecorder::Recorded(Arc::new(RwLock::new(EventRecordingPlayhead {
            events: io_events,
            used_events: HashSet::new(),
        })))
    }

    pub fn from_live_connection(pool: PgPool) -> Self {
        PgIoRecorder::Live(pool)
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

fn sqlx_error_to_recorder_error(e: sqlx::Error, query: &str, params: &[Parameter]) -> Error {
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
    ConnectionAcquireRequest,
    ConnectionAcquireResult(ConnectionAcquireResult),
    QueryRequest(QueryRequest),
    QueryResult(QueryResult),
    TxStartRequest,
    TxStartResult(TxStartResult),
    TxCommitRequest(TxCommitRequest),
    TxCommitResult(TxCommitResult),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConnectionAcquireResult(Result<(), Error>);

impl IoEvent {
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("io req should always be serializable")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryWithParameters {
    pub query_text: String,
    pub query_type: QueryType,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QueryType {
    Optional,
    RequiredSingle,
    Multiple,
    Execute,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QueryRequest {
    pub tx_id: Option<Uuid>,
    pub query_text: String,
    pub query_type: QueryType,
    pub parameters: Vec<Parameter>,
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
pub enum PgIoRecorder {
    Recorded(Arc<RwLock<EventRecordingPlayhead<IoEvent>>>),
    Live(sqlx::PgPool),
}

impl PgIoRecorder {
    pub async fn con(&self) -> Result<PgIoRecorderConnection, Error> {
        let io_req = IoEvent::ConnectionAcquireRequest;
        match self {
            PgIoRecorder::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event =
                    w_guard.get_io_event_response_marking_events_as_used(&io_req);
                drop(w_guard);
                let IoEvent::ConnectionAcquireResult(acquire_res) = recorded_response_event.value
                else {
                    panic!("unexpected response type")
                };
                acquire_res.0?;
                Ok(PgIoRecorderConnection::Recorded(Arc::clone(&recording)))
            }
            PgIoRecorder::Live(pool) => {
                let io_req = record_io_event_request_or_panic(RECORDER_NAME, io_req);
                let con_res = pool.acquire().await;
                return match con_res {
                    Ok(con) => {
                        io_req.record_response_serializing_and_panicking(
                            IoEvent::ConnectionAcquireResult(ConnectionAcquireResult(Ok(()))),
                            false,
                        );
                        Ok(PgIoRecorderConnection::Live(con))
                    }
                    Err(err) => {
                        let e = error_chain_to_pretty_formatted(err);
                        let err = Error::Internal {
                            msg: e,
                            location: Location::caller().to_string(),
                        };
                        io_req.record_response_serializing_and_panicking(
                            IoEvent::ConnectionAcquireResult(ConnectionAcquireResult(Err(
                                err.clone()
                            ))),
                            true,
                        );
                        Err(err)
                    }
                };
            }
        }
    }
}

pub enum PgIoRecorderConnection {
    Recorded(Arc<RwLock<EventRecordingPlayhead<IoEvent>>>),
    Live(sqlx::pool::PoolConnection<Postgres>),
}

pub struct Transaction<'a> {
    id: Uuid,
    tx: TransactionIoProvider<'a>,
}

pub enum TransactionIoProvider<'a> {
    Recorded(Arc<RwLock<EventRecordingPlayhead<IoEvent>>>),
    Live(sqlx::Transaction<'a, sqlx::Postgres>),
}
