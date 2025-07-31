use chrono::{DateTime, Utc};
use gel_protocol::value_opt::ValueOpt;
use gel_tokio::RawTransaction;
pub use parameters::Parameter;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::panic::Location;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing_config_helper::io_provider::execution_recorder::get_current_execution;
use tracing_config_helper::io_provider::{IoEventRequest, record_io_event_request, specialize_events_or_panic, EventRecordingPlayhead};
use tracked_error::error_chain_to_pretty_formatted;
use uuid::Uuid;
use tracing_config_helper::SpecializedIoEvent;

mod parameters;
pub const RECORDER_NAME: &str = "Gel";

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
        let io_events = tracing_config_helper::io_provider::get_io_provider_recording_events(RECORDER_NAME).expect("Gel events to exist if in recording");
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
        let recorded_io_req = record_io_event_request(RECORDER_NAME, request_event.as_json());
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
        recorded_io_req.record_response(raw_io_response.as_json(), is_error);
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
        parameters: HashMap<IntoString, Parameter>,
    ) -> Result<T, Error> {
        let parameters: HashMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let client = match &self {
            DatabaseIoRecorder::Recorded(_) => {
                unimplemented!()
            }
            DatabaseIoRecorder::Live(client) => client,
        };
        let io_req = IoEvent::QueryRequest(QueryRequest {
            tx_id: None,
            query_text: query.to_string(),
            query_type: QueryType::RequiredSingle,
            parameters: parameters.clone(),
        });
        let io_req_json = io_req.as_json();
        let recorded_io_req = record_io_event_request(RECORDER_NAME, io_req_json);
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
        parameters: HashMap<IntoString, Parameter>,
    ) -> Result<Option<T>, Error> {
        let parameters: HashMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let client = match &self {
            DatabaseIoRecorder::Recorded(_) => {
                unimplemented!()
            }
            DatabaseIoRecorder::Live(client) => client,
        };
        let io_request = IoEvent::QueryRequest(QueryRequest {
            tx_id: None,
            query_text: query.to_string(),
            query_type: QueryType::Optional,
            parameters: parameters.clone(),
        });
        let io_req_json = io_request.as_json();
        let recorded_io_req = record_io_event_request(RECORDER_NAME, io_req_json);
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
        parameters: HashMap<IntoString, Parameter>,
    ) -> Result<Vec<SerDe>, Error> {
        let parameters: HashMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let client = match &self {
            DatabaseIoRecorder::Recorded(_) => {
                unimplemented!()
            }
            DatabaseIoRecorder::Live(client) => client,
        };
        let io_event_req = IoEvent::QueryRequest(QueryRequest {
            tx_id: None,
            query_text: query.to_string(),
            query_type: QueryType::Multiple,
            parameters: parameters.clone(),
        });
        let io_event_req_json = io_event_req.as_json();
        let recorded_io_req = record_io_event_request(RECORDER_NAME, io_event_req_json);
        let raw_io_response: Result<Vec<SerDe>, Error> =
            raw_query_multiple(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());
        raw_io_response
    }
}

impl Transaction {
    // the returned id order is non-specified, so an order_by is required
    pub async fn bulk_insert(
        &mut self,
        table: &str,
        rows_columns: Vec<HashMap<String, Parameter>>,
        order_by: &str,
    ) -> Result<Vec<Uuid>, Error> {
        let Some(query) = gel::generate_bulk_insert_query(table, &rows_columns, order_by) else {
            return Ok(vec![]);
        };
        let params_as_json: serde_json::Value = bulk_params_as_json(&rows_columns);
        let params = HashMap::from([("data".to_string(), Parameter::from(params_as_json))]);
        let inserted_entity_id: Vec<Id> = self.query_multiple(&query, params).await?;
        if let Some(execution_external_id) = get_current_execution() {
            let mut bulk_insert_col = vec![];
            for (idx, columns) in rows_columns.into_iter().enumerate() {
                let mut cols = HashMap::new();
                let new = parameter_map_as_json_value(&columns);
                cols.insert("entity_name".to_string(), Parameter::from(table));
                cols.insert(
                    "entity_id".to_string(),
                    Parameter::from(
                        inserted_entity_id
                            .get(idx)
                            .expect("all elements to have been inserted")
                            .id,
                    ),
                );
                cols.insert(
                    "execution_external_id".to_string(),
                    Parameter::from(execution_external_id),
                );
                cols.insert("old".to_string(), Parameter::Json(None));
                cols.insert("new".to_string(), Parameter::from(new));
                bulk_insert_col.push(cols);
            }
            let query =
                gel::generate_bulk_insert_query("EntityChange", &bulk_insert_col, "id").unwrap();
            let params_as_json: serde_json::Value = bulk_params_as_json(&bulk_insert_col);
            let params = HashMap::from([("data".to_string(), Parameter::from(params_as_json))]);
            let _id: Vec<Id> = self.query_multiple(&query, params).await?;
        }
        Ok(inserted_entity_id.into_iter().map(|e| e.id).collect())
    }

    pub async fn insert<IntoString: Into<String>>(
        &mut self,
        table: &str,
        columns: HashMap<IntoString, Parameter>,
    ) -> Result<Uuid, Error> {
        let columns: HashMap<String, Parameter> =
            columns.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let query = gel::generate_insert_query(table, &columns);
        let inserted_entity_id: Id = self.query_required_single(&query, columns.clone()).await?;
        if let Some(execution_external_id) = get_current_execution() {
            let new = parameter_map_as_json_value(&columns);
            let query_with_params = generate_entity_change_query(
                execution_external_id,
                table,
                inserted_entity_id.id,
                None,
                new,
            );
            let _id: Id = self
                .query_required_single(&query_with_params.query_text, query_with_params.parameters)
                .await?;
        }
        Ok(inserted_entity_id.id)
    }

    // True if an entity was updated. It might not exist or the update data could be empty.
    pub async fn update<IntoString: Into<String>>(
        &mut self,
        table: &str,
        id: Uuid,
        columns: HashMap<IntoString, Parameter>,
    ) -> Result<bool, Error> {
        let columns: HashMap<String, Parameter> =
            columns.into_iter().map(|(k, v)| (k.into(), v)).collect();
        if columns.is_empty() {
            return Ok(false);
        }
        let keys = columns.keys().map(|k| k.clone()).collect::<Vec<String>>();
        let previous_state_query = gel::generate_select_query(table, id, &keys);
        let Some(old): Option<serde_json::Value> = self
            .query_optional(
                &previous_state_query,
                HashMap::<String, Parameter>::from([]),
            )
            .await?
        else {
            return Ok(false);
        };
        let update_query = gel::generate_update_query(table, id, &columns);
        let _id: Id = self
            .query_required_single(&update_query, columns.clone())
            .await?;
        let new = parameter_map_as_json_value(&columns);
        if let Some(execution_external_id) = get_current_execution() {
            let query_with_params =
                generate_entity_change_query(execution_external_id, table, id, Some(old), new);
            let _id: Id = self
                .query_required_single(&query_with_params.query_text, query_with_params.parameters)
                .await?;
        }
        Ok(true)
    }
    pub async fn query_required_single<
        IntoString: Into<String>,
        T: Serialize + DeserializeOwned + Clone,
    >(
        &mut self,
        query: &str,
        parameters: HashMap<IntoString, Parameter>,
    ) -> Result<T, Error> {
        let parameters: HashMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(_) => {
                unimplemented!()
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let io_req = IoEvent::QueryRequest(QueryRequest {
            tx_id: Some(self.id),
            query_text: query.to_string(),
            query_type: QueryType::RequiredSingle,
            parameters: parameters.clone(),
        });
        let recorded_io_req = record_io_event_request(RECORDER_NAME, io_req.as_json());
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
        parameters: HashMap<IntoString, Parameter>,
    ) -> Result<Option<T>, Error> {
        let parameters: HashMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(_) => {
                unimplemented!()
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let io_request = IoEvent::QueryRequest(QueryRequest {
            tx_id: Some(self.id),
            query_text: query.to_string(),
            query_type: QueryType::Optional,
            parameters: parameters.clone(),
        });
        let io_req_json = io_request.as_json();
        let recorded_io_req = record_io_event_request(RECORDER_NAME, io_req_json);
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
        parameters: HashMap<IntoString, Parameter>,
    ) -> Result<Vec<T>, Error> {
        let parameters: HashMap<String, Parameter> =
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

        let io_request_json = request_event.as_json();
        let recorded_io_req = record_io_event_request(RECORDER_NAME, io_request_json);
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
        let io_req_json = request_event.as_json();
        let recorded_io_req = record_io_event_request(RECORDER_NAME, io_req_json);
        let raw_io_response = tx.commit().await.map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            Error::Internal {
                msg: err_str,
                location: Location::caller().to_string(),
            }
        });
        let is_err = raw_io_response.is_err();
        let io_response = IoEvent::TxCommitResult(TxCommitResult(raw_io_response.clone()));
        let io_response_json = io_response.as_json();
        recorded_io_req.record_response(io_response_json, is_err);
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

impl IoEvent {
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("io req should always be serializable")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryWithParameters {
    pub query_text: String,
    pub query_type: QueryType,
    pub parameters: HashMap<String, Parameter>,
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
    pub parameters: HashMap<String, Parameter>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Id {
    pub id: Uuid,
}
mod gel;

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionResult {
    pub ended_at: DateTime<Utc>,
    pub result: Result<(), Error>,
}

pub struct Transaction {
    id: Uuid,
    tx: TransactionIoProvider,
}

fn parameter_map_as_json_value(columns: &HashMap<String, Parameter>) -> serde_json::Value {
    let mut new = serde_json::map::Map::new();
    for (k, v) in columns {
        new.insert(k.to_string(), v.as_json());
    }
    serde_json::Value::Object(new)
}

fn generate_entity_change_query(
    execution_external_id: Uuid,
    entity_name: &str,
    entity_id: Uuid,
    old: Option<serde_json::Value>,
    new: serde_json::Value,
) -> QueryWithParameters {
    let args = HashMap::from([
        ("entity_name".to_string(), Parameter::from(entity_name)),
        ("entity_id".to_string(), Parameter::from(entity_id)),
        (
            "execution_external_id".to_string(),
            Parameter::from(execution_external_id),
        ),
        ("old".to_string(), Parameter::from(old)),
        ("new".to_string(), Parameter::from(new)),
    ]);
    let query = "insert EntityChange{
    entity_name := <str>$entity_name,
    entity_id := <uuid>$entity_id,
    execution_external_id := <uuid>$execution_external_id,
    new := <json>$new
};";
    QueryWithParameters {
        query_text: query.to_string(),
        query_type: QueryType::RequiredSingle,
        parameters: args,
    }
}

fn bulk_params_as_json(columns: &Vec<HashMap<String, Parameter>>) -> serde_json::Value {
    let w: Vec<serde_json::Value> = columns
        .iter()
        .map(|e| parameter_map_as_json_value(&e))
        .collect();
    serde_json::Value::Array(w)
}

async fn raw_query_optional<T: Serialize + DeserializeOwned + Clone>(
    client: &gel_tokio::Client,
    query: &str,
    parameters: HashMap<String, Parameter>,
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
    parameters: HashMap<String, Parameter>,
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
    parameters: HashMap<String, Parameter>,
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
    parameters: HashMap<String, Parameter>,
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
    parameters: HashMap<String, Parameter>,
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
    parameters: HashMap<String, Parameter>,
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
    let io_response_json = io_response.as_json();
    recorded_io_req.record_response(io_response_json, is_err);
}
