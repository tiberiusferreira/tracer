use chrono::{DateTime, NaiveDate, Utc};
use gel_protocol::value_opt::ValueOpt;
use gel_tokio::RawTransaction;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::panic::Location;
use thiserror::Error;
use tracing_config_helper::io_provider::execution_recorder::{
    get_current_execution, get_global_collector,
};
use tracing_config_helper::io_provider::record_io_event_request;
use tracked_error::error_chain_to_pretty_formatted;
use uuid::Uuid;

pub const RECORDER_NAME: &str = "Gel";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryWithParameters {
    pub query_text: String,
    pub query_type: QueryType,
    pub parameters: HashMap<String, Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryType {
    Plain,
    RequiredSingle,
    Optional,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryRequest {
    pub id: Uuid,
    pub tx_id: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub query_with_parameters: QueryWithParameters,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryResult {
    pub id: Uuid,
    pub ended_at: DateTime<Utc>,
    pub result: Result<serde_json::Value, Error>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxCommit {
    pub id: Uuid,
    pub tx_id: Uuid,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxStartRequest {
    pub id: Uuid,
}

pub type TxId = Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxStartResult {
    pub id: Uuid,
    pub result: Result<TxId, Error>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxCommitResult {
    pub id: Uuid,
    pub tx_id: Uuid,
    pub result: Result<(), Error>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum IoEvent {
    QueryRequest(QueryRequest),
    QueryResult(QueryResult),
    TxStartRequest(TxStartRequest),
    TxStartResult(TxStartResult),
    TxCommitRequest(TxCommit),
    TxCommitResult(TxCommitResult),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Id {
    pub id: Uuid,
}
mod gel;

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
pub enum Error {
    #[error("Internal {msg} at {location}")]
    Internal { msg: String, location: String },
    #[error("Serde {msg} at {location}")]
    Serde { msg: String, location: String },
}

impl Error {
    #[track_caller]
    fn from_serde_json(query_text: &str, value: &str, err: serde_json::Error) -> Self {
        std::fs::File::create("error.txt")
            .unwrap()
            .write_all(value.as_bytes())
            .unwrap();
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Parameter {
    Uuid {
        val: Option<Uuid>,
        cast_to_table: Option<String>,
    },
    String(Option<String>),
    Date(Option<NaiveDate>),
    Datetime(Option<DateTime<Utc>>),
    Bool(Option<bool>),
    Json(Option<serde_json::Value>),
    I32(Option<i32>),
}

impl From<Uuid> for Parameter {
    fn from(value: Uuid) -> Self {
        Parameter::Uuid {
            val: Some(value),
            cast_to_table: None,
        }
    }
}

impl From<Option<Uuid>> for Parameter {
    fn from(value: Option<Uuid>) -> Self {
        Parameter::Uuid {
            val: value,
            cast_to_table: None,
        }
    }
}

impl From<(Uuid, &str)> for Parameter {
    fn from(value: (Uuid, &str)) -> Self {
        Parameter::Uuid {
            val: Some(value.0),
            cast_to_table: Some(value.1.to_string()),
        }
    }
}

impl From<bool> for Parameter {
    fn from(value: bool) -> Self {
        Parameter::Bool(Some(value))
    }
}

impl From<Option<bool>> for Parameter {
    fn from(value: Option<bool>) -> Self {
        Parameter::Bool(value)
    }
}

impl From<NaiveDate> for Parameter {
    fn from(value: NaiveDate) -> Self {
        Parameter::Date(Some(value))
    }
}

impl From<Option<NaiveDate>> for Parameter {
    fn from(value: Option<NaiveDate>) -> Self {
        Parameter::Date(value)
    }
}

impl From<DateTime<Utc>> for Parameter {
    fn from(value: DateTime<Utc>) -> Self {
        Parameter::Datetime(Some(value))
    }
}

impl From<Option<DateTime<Utc>>> for Parameter {
    fn from(value: Option<DateTime<Utc>>) -> Self {
        Parameter::Datetime(value)
    }
}

impl From<String> for Parameter {
    fn from(value: String) -> Self {
        Parameter::String(Some(value))
    }
}

impl From<Option<String>> for Parameter {
    fn from(value: Option<String>) -> Self {
        Parameter::String(value)
    }
}

impl From<&String> for Parameter {
    fn from(value: &String) -> Self {
        Parameter::String(Some(value.clone()))
    }
}

impl From<Option<&String>> for Parameter {
    fn from(value: Option<&String>) -> Self {
        Parameter::String(value.map(String::to_string))
    }
}

impl From<&str> for Parameter {
    fn from(value: &str) -> Self {
        Parameter::String(Some(value.to_string()))
    }
}

impl From<Option<&str>> for Parameter {
    fn from(value: Option<&str>) -> Self {
        Parameter::String(value.map(String::from))
    }
}

impl From<serde_json::Value> for Parameter {
    fn from(value: serde_json::Value) -> Self {
        Parameter::Json(Some(value))
    }
}

impl From<Option<serde_json::Value>> for Parameter {
    fn from(value: Option<serde_json::Value>) -> Self {
        Parameter::Json(value)
    }
}

impl From<i32> for Parameter {
    fn from(value: i32) -> Self {
        Parameter::I32(Some(value))
    }
}

impl From<Option<i32>> for Parameter {
    fn from(value: Option<i32>) -> Self {
        Parameter::I32(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionResult {
    pub ended_at: DateTime<Utc>,
    pub result: Result<(), Error>,
}

#[derive(Clone)]
pub struct ExecutionIoProvider {
    pub database: DatabaseIoRecorder,
}

impl ExecutionIoProvider {
    pub fn database(&self) -> &DatabaseIoRecorder {
        &self.database
    }
}

#[derive(Clone)]
pub enum DatabaseIoRecorder {
    Recorded(()),
    Live(gel_tokio::Client),
}

pub enum TransactionIoProvider {
    Recorded(()),
    Live(RawTransaction),
}

pub struct Transaction {
    id: Uuid,
    tx: TransactionIoProvider,
}

impl Transaction {
    pub async fn insert(
        &mut self,
        table: &str,
        columns: HashMap<&str, Parameter>,
    ) -> Result<Uuid, Error> {
        let columns: HashMap<String, Parameter> = columns
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let query = gel::generate_insert_query(table, &columns);
        let id: Id = self.query_required_single(&query, columns.clone()).await?;
        let mut new = serde_json::map::Map::new();
        for (k, v) in &columns {
            match v {
                Parameter::Uuid { val, .. } => {
                    new.insert(
                        k.to_string(),
                        serde_json::Value::from(val.map(|v| v.to_string())),
                    );
                }
                Parameter::String(val) => {
                    new.insert(k.to_string(), serde_json::to_value(val).unwrap());
                }
                Parameter::I32(val) => {
                    new.insert(k.to_string(), serde_json::to_value(val).unwrap());
                }
                Parameter::Json(val) => {
                    new.insert(k.to_string(), serde_json::to_value(val).unwrap());
                }
                Parameter::Datetime(val) => {
                    new.insert(k.to_string(), serde_json::to_value(val).unwrap());
                }
                Parameter::Bool(val) => {
                    new.insert(k.to_string(), serde_json::to_value(val).unwrap());
                }
                Parameter::Date(val) => {
                    new.insert(k.to_string(), serde_json::to_value(val).unwrap());
                }
            }
        }
        let new = serde_json::Value::Object(new);

        let execution_external_id = get_current_execution().unwrap();
        let args = HashMap::from([
            (
                "entity_name".to_string(),
                Parameter::from(table.to_string()),
            ),
            ("entity_id".to_string(), Parameter::from(id.id)),
            (
                "execution_external_id".to_string(),
                Parameter::from(execution_external_id),
            ),
            ("new".to_string(), Parameter::from(new)),
        ]);

        let _id: Id = self
            .query_required_single(
                "insert EntityChange{
    entity_name := <str>$entity_name,
    entity_id := <uuid>$entity_id,
    execution_external_id := <uuid>$execution_external_id,
    new := <json>$new
};",
                args,
            )
            .await?;
        Ok(id.id)
    }
    pub async fn query_required_single<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        query: &str,
        parameters: HashMap<String, Parameter>,
    ) -> Result<T, Error> {
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(_) => {
                unimplemented!()
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let query_id = Uuid::new_v4();
        let event = IoEvent::QueryRequest(QueryRequest {
            id: query_id,
            tx_id: Some(self.id),
            started_at: Utc::now(),
            query_with_parameters: QueryWithParameters {
                query_text: query.to_string(),
                query_type: QueryType::RequiredSingle,
                parameters: parameters.clone(),
            },
        });
        let request = record_io_event_request(RECORDER_NAME, serde_json::to_value(&event).unwrap());
        let result: Result<T, Error> = run_tx_query_required(client, query, parameters).await;
        let result_json = result
            .clone()
            .map(|value| serde_json::to_value(&value).unwrap());
        let is_err = result_json.is_err();
        let event_result = IoEvent::QueryResult(QueryResult {
            id: query_id,
            ended_at: Utc::now(),
            result: result_json,
        });
        let event_result_json = serde_json::to_value(&event_result).unwrap();
        request.record_response(event_result_json, is_err);

        result
    }

    pub async fn query_optional<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        query: &str,
        parameters: HashMap<String, Parameter>,
    ) -> Result<Option<T>, Error> {
        let execution_id = get_current_execution().unwrap();
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(_) => {
                unimplemented!()
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let global_collector = get_global_collector();
        let query_id = Uuid::new_v4();
        let event = IoEvent::QueryRequest(QueryRequest {
            id: query_id,
            tx_id: Some(self.id),
            started_at: Utc::now(),
            query_with_parameters: QueryWithParameters {
                query_text: query.to_string(),
                query_type: QueryType::Optional,
                parameters: parameters.clone(),
            },
        });
        let event_json = serde_json::to_value(&event).unwrap();
        let event_id =
            global_collector.record_io_event(execution_id, RECORDER_NAME, event_json, false, None);
        let result: Result<Option<T>, Error> =
            run_tx_query_optional(client, query, parameters).await;
        let result_json = result
            .clone()
            .map(|value| serde_json::to_value(&value).unwrap());
        let is_err = result_json.is_err();
        let event_result = IoEvent::QueryResult(QueryResult {
            id: query_id,
            ended_at: Utc::now(),
            result: result_json,
        });
        let event_result_json = serde_json::to_value(&event_result).unwrap();
        global_collector.record_io_event(
            execution_id,
            RECORDER_NAME,
            event_result_json,
            is_err,
            Some(event_id),
        );
        result
    }

    pub async fn query<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        query: &str,
        parameters: HashMap<String, Parameter>,
    ) -> Result<T, Error> {
        let execution_id = get_current_execution().unwrap();
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(_) => {
                unimplemented!()
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let global_collector = get_global_collector();
        let query_id = Uuid::new_v4();
        let event = IoEvent::QueryRequest(QueryRequest {
            id: query_id,
            tx_id: Some(self.id),
            started_at: Utc::now(),
            query_with_parameters: QueryWithParameters {
                query_text: query.to_string(),
                query_type: QueryType::Plain,
                parameters: parameters.clone(),
            },
        });
        let event_json = serde_json::to_value(&event).unwrap();
        let event_id =
            global_collector.record_io_event(execution_id, RECORDER_NAME, event_json, false, None);
        let result: Result<T, Error> = run_query_tx(client, query, parameters).await;
        let result_json = result
            .clone()
            .map(|value| serde_json::to_value(&value).unwrap());
        let is_error = result_json.is_err();
        let event_result = IoEvent::QueryResult(QueryResult {
            id: query_id,
            ended_at: Utc::now(),
            result: result_json,
        });
        let event_result_json = serde_json::to_value(&event_result).unwrap();
        global_collector.record_io_event(
            execution_id,
            RECORDER_NAME,
            event_result_json,
            is_error,
            Some(event_id),
        );
        result
    }
    pub async fn commit(self) -> Result<(), Error> {
        let execution_id = get_current_execution().unwrap();
        let global_collector = get_global_collector();
        let tx = match self.tx {
            TransactionIoProvider::Recorded(_) => {
                unimplemented!()
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let id = uuid::Uuid::new_v4();
        let event = IoEvent::TxCommitRequest(TxCommit { id, tx_id: self.id });
        let event_id = global_collector.record_io_event(
            execution_id,
            RECORDER_NAME,
            serde_json::to_value(&event).unwrap(),
            false,
            None,
        );
        let res = tx.commit().await.map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            Error::Internal {
                msg: err_str,
                location: std::panic::Location::caller().to_string(),
            }
        });
        let is_error = res.is_err();
        let event_result = IoEvent::TxCommitResult(TxCommitResult {
            id,
            tx_id: self.id,
            result: res.clone(),
        });
        global_collector.record_io_event(
            execution_id,
            RECORDER_NAME,
            serde_json::to_value(&event_result).unwrap(),
            is_error,
            Some(event_id),
        );
        res
    }
}
impl DatabaseIoRecorder {
    pub async fn transaction_start(&self) -> Transaction {
        let execution_id = get_current_execution().unwrap();
        let client = match &self {
            DatabaseIoRecorder::Recorded(_) => {
                unimplemented!()
            }
            DatabaseIoRecorder::Live(client) => client,
        };
        let global_collector = get_global_collector();
        let id = Uuid::new_v4();
        let event = IoEvent::TxStartRequest(TxStartRequest { id });
        let event_id = global_collector.record_io_event(
            execution_id,
            RECORDER_NAME,
            serde_json::to_value(&event).unwrap(),
            false,
            None,
        );
        let tx = client.transaction_raw().await.map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            let err_str = format!("{err_str} start tx");
            Error::Internal {
                msg: err_str,
                location: std::panic::Location::caller().to_string(),
            }
        });
        let tx_id = Uuid::new_v4();
        let result = if let Err(e) = &tx {
            Err(e.clone())
        } else {
            Ok(tx_id)
        };
        let is_error = result.is_err();
        let event_result = IoEvent::TxStartResult(TxStartResult { id, result });
        global_collector.record_io_event(
            execution_id,
            RECORDER_NAME,
            serde_json::to_value(&event_result).unwrap(),
            is_error,
            Some(event_id),
        );
        Transaction {
            id: tx_id,
            tx: TransactionIoProvider::Live(tx.unwrap()),
        }
    }

    pub async fn query<T: Serialize + DeserializeOwned + Clone, AsStr: AsRef<str>>(
        &self,
        query: &str,
        parameters: HashMap<AsStr, Parameter>,
    ) -> Result<T, Error> {
        let parameters: HashMap<String, Parameter> = parameters
            .into_iter()
            .map(|(k, v)| (k.as_ref().to_string(), v))
            .collect();
        let execution_id = get_current_execution().unwrap();
        let client = match &self {
            DatabaseIoRecorder::Recorded(_) => {
                unimplemented!()
            }
            DatabaseIoRecorder::Live(client) => client,
        };
        let global_collector = get_global_collector();
        let id = Uuid::new_v4();
        let event = IoEvent::QueryRequest(QueryRequest {
            id,
            tx_id: None,
            started_at: Utc::now(),
            query_with_parameters: QueryWithParameters {
                query_text: query.to_string(),
                query_type: QueryType::Plain,
                parameters: parameters.clone(),
            },
        });
        let event_id = global_collector.record_io_event(
            execution_id,
            RECORDER_NAME,
            serde_json::to_value(&event).unwrap(),
            false,
            None,
        );
        let result: Result<T, Error> = run_query(client, query, parameters).await;
        let res_as_json_value = result.clone().map(|v| serde_json::to_value(v).unwrap());
        let is_error = res_as_json_value.is_err();
        let event_result = IoEvent::QueryResult(QueryResult {
            id,
            ended_at: Utc::now(),
            result: res_as_json_value,
        });
        global_collector.record_io_event(
            execution_id,
            RECORDER_NAME,
            serde_json::to_value(&event_result).unwrap(),
            is_error,
            Some(event_id),
        );
        result
    }
}

async fn run_query<T: Serialize + DeserializeOwned + Clone, S: Into<String>>(
    client: &gel_tokio::Client,
    query: &str,
    parameters: HashMap<S, Parameter>,
) -> Result<T, Error> {
    let parameters: HashMap<String, Parameter> =
        parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
    let gel_params = gel::params_to_gel(parameters);
    let gel_params: HashMap<&str, ValueOpt> = gel_params
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    let query_result: gel_protocol::model::Json =
        client.query_json(query, &gel_params).await.map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            let err_str = format!("{err_str} with query {}", query);
            Error::Internal {
                msg: err_str,
                location: Location::caller().to_string(),
            }
        })?;
    let query_result: T = serde_json::from_str(&query_result)
        .map_err(|e| Error::from_serde_json(query, &query_result, e))?;

    Ok(query_result)
}

async fn run_query_tx<T: Serialize + DeserializeOwned + Clone>(
    client: &mut RawTransaction,
    query: &str,
    parameters: HashMap<String, Parameter>,
) -> Result<T, Error> {
    let gel_params = gel::params_to_gel(parameters);
    let gel_params: HashMap<&str, ValueOpt> = gel_params
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();

    let query_result: gel_protocol::model::Json =
        client.query_json(query, &gel_params).await.map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            let err_str = format!("{err_str} with query {}", query);
            Error::Internal {
                msg: err_str,
                location: std::panic::Location::caller().to_string(),
            }
        })?;

    let query_result: T = serde_json::from_str(&query_result)
        .map_err(|e| Error::from_serde_json(query, &query_result, e))?;
    Ok(query_result)
}

async fn run_tx_query_required<T: Serialize + DeserializeOwned + Clone>(
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
        .map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            let err_str = format!("{err_str} with query {}", query);
            Error::Internal {
                msg: err_str,
                location: Location::caller().to_string(),
            }
        })?;
    let query_result: T = serde_json::from_str(&query_result)
        .map_err(|e| Error::from_serde_json(query, &query_result, e))?;
    Ok(query_result)
}

async fn run_tx_query_optional<T: Serialize + DeserializeOwned + Clone>(
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
        .map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            let err_str = format!("{err_str} with query {}", query);
            Error::Internal {
                msg: err_str,
                location: std::panic::Location::caller().to_string(),
            }
        })?;
    let query_result = match query_result {
        None => return Ok(None),
        Some(query_result) => query_result,
    };
    let query_result: T = serde_json::from_str(&query_result)
        .map_err(|e| Error::from_serde_json(query, &query_result, e))?;
    Ok(Some(query_result))
}
