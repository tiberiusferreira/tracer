use gel_protocol::value_opt::ValueOpt;
use gel_tokio::RawTransaction;
pub use parameters::Parameter;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::panic::Location;
use std::sync::{Arc, RwLock};
use indexmap::IndexMap;
use thiserror::Error;
use tracer::recorder_api::{record_io_event_request_or_panic, IoEventRequest};
use tracer::player_api::{SpecializedIoEvent, EventRecordingPlayhead, get_io_provider_recorded_events, specialize_events_or_panic};
use tracked_error::error_chain_to_pretty_formatted;
use uuid::Uuid;

mod parameters;
mod gel;
pub const RECORDER_NAME: &str = "Gel";

pub trait ToParameters {
    fn to_parameters(&self) -> IndexMap<String, Parameter>;
}

#[derive(Clone)]
pub enum DatabaseIoRecorder {
    Recorded(Arc<RwLock<EventRecordingPlayhead<IoEvent>>>),
    Live(gel_tokio::Client),
}

pub enum TransactionIoProvider {
    Recorded(Arc<RwLock<EventRecordingPlayhead<IoEvent>>>),
    Live(RawTransaction),
}


impl DatabaseIoRecorder {
    pub fn from_global_recording() -> Self {
        let io_events = get_io_provider_recorded_events(RECORDER_NAME).expect("Gel events to exist if in recording");
        let io_events: Vec<SpecializedIoEvent<IoEvent>> = specialize_events_or_panic(io_events);
        DatabaseIoRecorder::Recorded(Arc::new(RwLock::new(EventRecordingPlayhead { events: io_events, used_events: HashSet::new() })))
    }
    pub async fn transaction_start(&self) -> Result<Transaction, Error> {
        let request_event = IoEvent::TxStartRequest;
        let client = match &self {
            DatabaseIoRecorder::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::TxStartResult(TxStartResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                let tx_id = result?;
                return Ok(Transaction {
                    id: tx_id,
                    tx: TransactionIoProvider::Recorded(Arc::clone(&recording)),
                });
            }
            DatabaseIoRecorder::Live(client) => client,
        };
        let recorded_io_req = record_io_event_request_or_panic(RECORDER_NAME, request_event);
        let tx = client.transaction_raw().await.map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            let err_str = format!("{err_str} start tx");
            Error::Internal {
                msg: err_str,
                location: Location::caller().to_string(),
            }
        });
        let tx_id = Uuid::new_v4();
        let result = if let Err(e) = &tx {
            Err(e.clone())
        } else {
            Ok(tx_id)
        };
        let is_error = result.is_err();
        let raw_io_response = IoEvent::TxStartResult(TxStartResult(result));
        recorded_io_req.record_response_serializing_and_panicking(raw_io_response, is_error);
        Ok(Transaction {
            id: tx_id,
            tx: TransactionIoProvider::Live(tx?),
        })
    }

    pub async fn query_required_single<
        IntoString: Into<String>,
        T: Serialize + DeserializeOwned + Clone,
    >(
        &mut self,
        query: &str,
        parameters: IndexMap<IntoString, Parameter>,
    ) -> Result<T, Error> {
        let parameters: IndexMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let request_event = IoEvent::QueryRequest(QueryRequest {
            tx_id: None,
            query_text: query.to_string(),
            query_type: QueryType::RequiredSingle,
            parameters: parameters.clone(),
        });
        let client = match &self {
            DatabaseIoRecorder::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::QueryResult(QueryResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                let res = result?;
                let res: T = serde_json::from_value(res).expect("result was not the correct type");
                return Ok(res);
            }
            DatabaseIoRecorder::Live(client) => client,
        };
        let recorded_io_req = record_io_event_request_or_panic(RECORDER_NAME, request_event);
        let raw_io_response: Result<T, Error> = raw_query_required(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());
        raw_io_response
    }

    pub async fn query_optional<
        IntoString: Into<String>,
        T: Serialize + DeserializeOwned + Clone,
    >(
        &mut self,
        query: &str,
        parameters: IndexMap<IntoString, Parameter>,
    ) -> Result<Option<T>, Error> {
        let parameters: IndexMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let request_event = IoEvent::QueryRequest(QueryRequest {
            tx_id: None,
            query_text: query.to_string(),
            query_type: QueryType::Optional,
            parameters: parameters.clone(),
        });
        let client = match &self {
            DatabaseIoRecorder::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::QueryResult(QueryResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                let res = result?;
                let res: Option<T> = serde_json::from_value(res).expect("result was not the correct type");
                return Ok(res);
            }
            DatabaseIoRecorder::Live(client) => client,
        };
        let recorded_io_req = record_io_event_request_or_panic(RECORDER_NAME, request_event);
        let raw_io_response: Result<Option<T>, Error> =
            raw_query_optional(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());
        raw_io_response
    }

    pub async fn query_multiple<
        SerDe: Serialize + DeserializeOwned + Clone,
        IntoString: Into<String>,
    >(
        &self,
        query: &str,
        parameters: IndexMap<IntoString, Parameter>,
    ) -> Result<Vec<SerDe>, Error> {
        let parameters: IndexMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let request_event = IoEvent::QueryRequest(QueryRequest {
            tx_id: None,
            query_text: query.to_string(),
            query_type: QueryType::Multiple,
            parameters: parameters.clone(),
        });
        let client = match &self {
            DatabaseIoRecorder::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::QueryResult(QueryResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                let res = result?;
                let res: Vec<SerDe> = serde_json::from_value(res).expect("result was not the correct type");
                return Ok(res);
            }
            DatabaseIoRecorder::Live(client) => client,
        };
        let recorded_io_req = record_io_event_request_or_panic(RECORDER_NAME, request_event);
        let raw_io_response: Result<Vec<SerDe>, Error> =
            raw_query_multiple(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());
        raw_io_response
    }
}

impl Transaction {
    pub async fn query_required_single<
        IntoString: Into<String>,
        T: Serialize + DeserializeOwned + Clone,
    >(
        &mut self,
        query: &str,
        parameters: IndexMap<IntoString, Parameter>,
    ) -> Result<T, Error> {
        let parameters: IndexMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let request_event = IoEvent::QueryRequest(QueryRequest {
            tx_id: Some(self.id),
            query_text: query.to_string(),
            query_type: QueryType::RequiredSingle,
            parameters: parameters.clone(),
        });
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::QueryResult(QueryResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                let res = result?;
                let res: T = serde_json::from_value(res).expect("result was not the correct type");
                return Ok(res);
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let recorded_io_req = record_io_event_request_or_panic(RECORDER_NAME, request_event);
        let raw_io_response: Result<T, Error> =
            raw_tx_query_required(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());
        raw_io_response
    }

    pub async fn query_optional<
        IntoString: Into<String>,
        T: Serialize + DeserializeOwned + Clone,
    >(
        &mut self,
        query: &str,
        parameters: IndexMap<IntoString, Parameter>,
    ) -> Result<Option<T>, Error> {
        let parameters: IndexMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let request_event = IoEvent::QueryRequest(QueryRequest {
            tx_id: Some(self.id),
            query_text: query.to_string(),
            query_type: QueryType::Optional,
            parameters: parameters.clone(),
        });
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::QueryResult(QueryResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                let res = result?;
                let res: Option<T> = serde_json::from_value(res).expect("result was not the correct type");
                return Ok(res);
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let recorded_io_req = record_io_event_request_or_panic(RECORDER_NAME, request_event);
        let raw_io_response: Result<Option<T>, Error> =
            raw_tx_query_optional(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());
        raw_io_response
    }

    pub async fn query_multiple<
        IntoString: Into<String>,
        T: Serialize + DeserializeOwned + Clone,
    >(
        &mut self,
        query: &str,
        parameters: IndexMap<IntoString, Parameter>,
    ) -> Result<Vec<T>, Error> {
        let parameters: IndexMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let request_event = IoEvent::QueryRequest(QueryRequest {
            tx_id: Some(self.id),
            query_text: query.to_string(),
            query_type: QueryType::Multiple,
            parameters: parameters.clone(),
        });
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::QueryResult(QueryResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                let json_response = result?;
                let result: Vec<T> = serde_json::from_value(json_response).expect("result was not the correct type");

                return Ok(result);
            }
            TransactionIoProvider::Live(tx) => tx,
        };

        let recorded_io_req = record_io_event_request_or_panic(RECORDER_NAME, request_event);
        let raw_io_response: Result<Vec<T>, Error> =
            raw_tx_query_multiple(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());

        raw_io_response
    }
    pub async fn commit(self) -> Result<(), Error> {
        let request_event = IoEvent::TxCommitRequest(TxCommitRequest { tx_id: self.id });
        let tx = match self.tx {
            TransactionIoProvider::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::TxCommitResult(TxCommitResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                return result;
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let recorded_io_req = record_io_event_request_or_panic(RECORDER_NAME, request_event);
        let raw_io_response = tx.commit().await.map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            Error::Internal {
                msg: err_str,
                location: Location::caller().to_string(),
            }
        });
        let is_err = raw_io_response.is_err();
        let io_response = IoEvent::TxCommitResult(TxCommitResult(raw_io_response.clone()));
        recorded_io_req.record_response_serializing_and_panicking(io_response, is_err);
        raw_io_response
    }
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


#[derive(Debug, Clone, Serialize, Deserialize, Error, PartialEq)]
pub enum Error {
    #[error("Internal {msg} at {location}")]
    Internal { msg: String, location: String },
    #[error("Serde {msg} at {location}")]
    Serde { msg: String, location: String },
}

impl Error {
    #[track_caller]
    fn from_serde_json(query_text: &str, value: &str, err: serde_json::Error) -> Self {
        let err = error_chain_to_pretty_formatted(&err);
        let error = format!(
            "Error: {err}\n\nQuery:\n\n{query_text}\n\nValue being deserialized:\n\n{value}"
        );
        Error::Serde {
            msg: error,
            location: Location::caller().to_string(),
        }
    }
}


pub struct Transaction {
    id: Uuid,
    tx: TransactionIoProvider,
}


async fn raw_query_optional<T: Serialize + DeserializeOwned + Clone>(
    client: &gel_tokio::Client,
    query: &str,
    parameters: IndexMap<String, Parameter>,
) -> Result<Option<T>, Error> {
    let gel_params = gel::params_to_gel(parameters);
    let gel_params: HashMap<&str, ValueOpt> = gel_params
        .iter()
        .map(|(k, v)| (k.as_ref(), v.clone()))
        .collect();
    let query_result: Option<gel_protocol::model::Json> = client
        .query_single_json(query, &gel_params)
        .await
        .map_err(|e| gel_error_to_recorder_error(e, query, &gel_params))?;
    let query_result = match query_result {
        None => return Ok(None),
        Some(query_result) => query_result,
    };
    let query_result: T = serde_json::from_str(&query_result)
        .map_err(|e| Error::from_serde_json(query, &query_result, e))?;
    Ok(Some(query_result))
}

async fn raw_query_required<T: Serialize + DeserializeOwned + Clone>(
    client: &gel_tokio::Client,
    query: &str,
    parameters: IndexMap<String, Parameter>,
) -> Result<T, Error> {
    let gel_params = gel::params_to_gel(parameters);
    let gel_params: HashMap<&str, ValueOpt> = gel_params
        .iter()
        .map(|(k, v)| (k.as_ref(), v.clone()))
        .collect();
    let query_result: gel_protocol::model::Json = client
        .query_required_single(query, &gel_params)
        .await
        .map_err(|e| gel_error_to_recorder_error(e, query, &gel_params))?;
    let query_result: T = serde_json::from_str(&query_result)
        .map_err(|e| Error::from_serde_json(query, &query_result, e))?;
    Ok(query_result)
}
async fn raw_query_multiple<T: Serialize + DeserializeOwned + Clone>(
    client: &gel_tokio::Client,
    query: &str,
    parameters: IndexMap<String, Parameter>,
) -> Result<Vec<T>, Error> {
    let gel_params = gel::params_to_gel(parameters);
    let gel_params: HashMap<&str, ValueOpt> = gel_params
        .iter()
        .map(|(k, v)| (k.as_ref(), v.clone()))
        .collect();
    let query_result: gel_protocol::model::Json = client
        .query_json(query, &gel_params)
        .await
        .map_err(|e| gel_error_to_recorder_error(e, query, &gel_params))?;
    let query_result: Vec<T> = serde_json::from_str(&query_result)
        .map_err(|e| Error::from_serde_json(query, &query_result, e))?;
    Ok(query_result)
}

async fn raw_tx_query_optional<T: Serialize + DeserializeOwned + Clone>(
    client: &mut RawTransaction,
    query: &str,
    parameters: IndexMap<String, Parameter>,
) -> Result<Option<T>, Error> {
    let gel_params = gel::params_to_gel(parameters);
    let gel_params: HashMap<&str, ValueOpt> = gel_params
        .iter()
        .map(|(k, v)| (k.as_ref(), v.clone()))
        .collect();

    let query_result: Option<gel_protocol::model::Json> = client
        .query_single_json(query, &gel_params)
        .await
        .map_err(|e| gel_error_to_recorder_error(e, query, &gel_params))?;
    let query_result = match query_result {
        None => return Ok(None),
        Some(query_result) => query_result,
    };
    let query_result: T = serde_json::from_str(&query_result)
        .map_err(|e| Error::from_serde_json(query, &query_result, e))?;
    Ok(Some(query_result))
}

async fn raw_tx_query_required<T: Serialize + DeserializeOwned + Clone>(
    client: &mut RawTransaction,
    query: &str,
    parameters: IndexMap<String, Parameter>,
) -> Result<T, Error> {
    let gel_params = gel::params_to_gel(parameters);
    let gel_params: HashMap<&str, ValueOpt> = gel_params
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();

    let query_result: gel_protocol::model::Json = client
        .query_required_single_json(query, &gel_params)
        .await
        .map_err(|e| gel_error_to_recorder_error(e, query, &gel_params))?;
    let query_result: T = serde_json::from_str(&query_result)
        .map_err(|e| Error::from_serde_json(query, &query_result, e))?;
    Ok(query_result)
}

async fn raw_tx_query_multiple<T: Serialize + DeserializeOwned + Clone>(
    client: &mut RawTransaction,
    query: &str,
    parameters: IndexMap<String, Parameter>,
) -> Result<Vec<T>, Error> {
    let gel_params = gel::params_to_gel(parameters);
    let gel_params: HashMap<&str, ValueOpt> = gel_params
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();

    let query_result: gel_protocol::model::Json = client
        .query_json(query, &gel_params)
        .await
        .map_err(|e| gel_error_to_recorder_error(e, query, &gel_params))?;

    let query_result: Vec<T> = serde_json::from_str(&query_result)
        .map_err(|e| Error::from_serde_json(query, &query_result, e))?;
    Ok(query_result)
}

fn gel_error_to_recorder_error(
    e: gel_tokio::Error,
    query: &str,
    gel_params: &HashMap<&str, ValueOpt>,
) -> Error {
    let err_str = error_chain_to_pretty_formatted(&e);
    let err_str = format!("{err_str} with query:\n{query}\nand parameters\n{gel_params:#?}");
    Error::Internal {
        msg: err_str,
        location: Location::caller().to_string(),
    }
}

fn record_io_response_as_query_result<T: Serialize>(
    recorded_io_req: IoEventRequest,
    raw_io_response: Result<T, Error>,
) {
    let raw_io_response_json = raw_io_response.map(|value| {
        serde_json::to_value(&value).expect("gel response should always be serializable")
    });
    let is_err = raw_io_response_json.is_err();
    let io_response = IoEvent::QueryResult(QueryResult(raw_io_response_json));
    recorded_io_req.record_response_serializing_and_panicking(io_response, is_err);
}
