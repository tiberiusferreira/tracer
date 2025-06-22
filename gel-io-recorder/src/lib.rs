use chrono::{DateTime, NaiveDate, Utc};
use gel_protocol::value_opt::ValueOpt;
use gel_tokio::RawTransaction;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::panic::Location;
use thiserror::Error;
use tracing_config_helper::io_provider::execution_recorder::get_current_execution;
use tracing_config_helper::io_provider::{IoEventRequest, record_io_event_request};
use tracked_error::error_chain_to_pretty_formatted;
use uuid::Uuid;

pub const RECORDER_NAME: &str = "Gel";

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryType {
    Optional,
    RequiredSingle,
    Multiple,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryRequest {
    pub tx_id: Option<Uuid>,
    pub query_text: String,
    pub query_type: QueryType,
    pub parameters: HashMap<String, Parameter>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryResult(Result<serde_json::Value, Error>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxCommitRequest {
    pub tx_id: Uuid,
}

pub type TxId = Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxStartResult(Result<TxId, Error>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxCommitResult(Result<(), Error>);

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

impl Parameter {
    pub fn as_json(&self) -> serde_json::Value {
        let err = "parameter serialization should never fail";
        match self {
            Parameter::Uuid { val, .. } => serde_json::Value::from(val.map(|v| v.to_string())),
            Parameter::String(val) => serde_json::to_value(val).expect(err),
            Parameter::I32(val) => serde_json::to_value(val).expect(err),
            Parameter::Json(val) => serde_json::to_value(val).expect(err),
            Parameter::Datetime(val) => serde_json::to_value(val).expect(err),
            Parameter::Bool(val) => serde_json::to_value(val).expect(err),
            Parameter::Date(val) => serde_json::to_value(val).expect(err),
        }
    }
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

impl Transaction {
    pub async fn bulk_insert(
        &mut self,
        table: &str,
        rows_columns: Vec<HashMap<String, Parameter>>,
    ) -> Result<Vec<Uuid>, Error> {
        let Some(query) = gel::generate_bulk_insert_query(table, &rows_columns) else {
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
            let query = gel::generate_bulk_insert_query("EntityChange", &bulk_insert_col).unwrap();
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
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(_) => {
                unimplemented!()
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let io_request = IoEvent::QueryRequest(QueryRequest {
            tx_id: Some(self.id),
            query_text: query.to_string(),
            query_type: QueryType::Multiple,
            parameters: parameters.clone(),
        });
        let io_request_json = io_request.as_json();
        let recorded_io_req = record_io_event_request(RECORDER_NAME, io_request_json);
        let raw_io_response: Result<Vec<T>, Error> =
            raw_tx_query_multiple(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());

        raw_io_response
    }
    pub async fn commit(self) -> Result<(), Error> {
        let tx = match self.tx {
            TransactionIoProvider::Recorded(_) => {
                unimplemented!()
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let io_req = IoEvent::TxCommitRequest(TxCommitRequest { tx_id: self.id });
        let io_req_json = io_req.as_json();
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
impl DatabaseIoRecorder {
    pub async fn transaction_start(&self) -> Result<Transaction, Error> {
        let client = match &self {
            DatabaseIoRecorder::Recorded(_) => {
                unimplemented!()
            }
            DatabaseIoRecorder::Live(client) => client,
        };
        let event = IoEvent::TxStartRequest;
        let recorded_io_req = record_io_event_request(RECORDER_NAME, event.as_json());
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
