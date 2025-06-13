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
        val: Uuid,
        cast_to_table: Option<String>,
    },
    String(String),
    NoneString,
    Date(NaiveDate),
    Datetime(DateTime<Utc>),
    Bool(bool),
    Json(serde_json::Value),
    I32(i32),
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
                    new.insert(k.to_string(), serde_json::Value::String(val.to_string()));
                }
                Parameter::String(val) => {
                    new.insert(k.to_string(), serde_json::Value::String(val.to_string()));
                }
                Parameter::I32(val) => {
                    new.insert(
                        k.to_string(),
                        serde_json::Value::Number(serde_json::Number::from(*val)),
                    );
                }
                Parameter::Json(val) => {
                    new.insert(k.to_string(), val.clone());
                }
                Parameter::Datetime(val) => {
                    new.insert(k.to_string(), serde_json::Value::String(val.to_string()));
                }
                Parameter::Bool(val) => {
                    new.insert(k.to_string(), serde_json::Value::Bool(*val));
                }
                Parameter::Date(val) => {
                    new.insert(k.to_string(), serde_json::Value::String(val.to_string()));
                }
                Parameter::NoneString => {
                    new.insert(k.to_string(), serde_json::Value::Null);
                }
            }
        }
        let new = serde_json::Value::Object(new);

        let execution_id = get_current_execution().unwrap();
        let args = HashMap::from([
            (
                "entity_name".to_string(),
                Parameter::String(table.to_string()),
            ),
            (
                "entity_id".to_string(),
                Parameter::Uuid {
                    val: id.id,
                    cast_to_table: None,
                },
            ),
            (
                "execution_id".to_string(),
                Parameter::Uuid {
                    val: execution_id,
                    cast_to_table: None,
                },
            ),
            ("new".to_string(), Parameter::Json(new)),
        ]);

        let _id: Id = self
            .query_required_single(
                "insert EntityChange{
    entity_name := <str>$entity_name,
    entity_id := <uuid>$entity_id,
    execution := <uuid>$execution_id,
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

    // pub async fn query_required_single<T: Serialize + DeserializeOwned + Clone>(
    //     &self,
    //     query: &str,
    //     parameters: HashMap<String, Parameter>,
    // ) -> Result<T, Error> {
    //     let execution_id = get_current_execution().unwrap();
    //     let client = match &self {
    //         DatabaseIoRecorder::Recorded(_) => {
    //             unimplemented!()
    //         }
    //         DatabaseIoRecorder::Live(client) => client,
    //     };
    //     let global_collector = get_global_collector();
    //     unimplemented!()
    // let query_id = global_collector.standalone_query_start(
    //     execution_id,
    //     QueryWithParameters {
    //         query_text: query.to_string(),
    //         parameters: parameters.clone(),
    //     },
    // );
    // let result: Result<T, Error> = run_query_required_single(client, query, parameters).await;
    // let res_as_json_value = result.clone().map(|v| serde_json::to_value(v).unwrap());
    // global_collector.standalone_query_end(execution_id, query_id, res_as_json_value);
    // result
    // }
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

async fn run_query<T: Serialize + DeserializeOwned + Clone>(
    client: &gel_tokio::Client,
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
                location: std::panic::Location::caller().to_string(),
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
        .map(|(k, v)| (k.as_str(), v.clone()))
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

/*

    pub fn record_generic_io_input(
        &self,
        execution_id: Uuid,
        io_provider: &str,
        function_name: &str,
        input: serde_json::Value,
    ) -> Uuid {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let id = Uuid::new_v4();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let recorded_ios = execution_context
            .replay_data
            .generic_io_providers
            .entry(io_provider.to_owned())
            .or_default();
        recorded_ios.push(GenericIoProviderIo {
            id,
            function_name: function_name.to_string(),
            started_at: Utc::now(),
            input,
            output: None,
        });
        id
    }
    pub fn record_generic_io_output(
        &self,
        execution_id: Uuid,
        io_provider: &str,
        io_id: Uuid,
        output: serde_json::Value,
    ) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let recorded_ios = execution_context
            .replay_data
            .generic_io_providers
            .get_mut(io_provider)
            .unwrap();
        let io = recorded_ios.iter_mut().find(|io| io.id == io_id).unwrap();
        assert!(io.output.is_none());
        io.output = Some(GenericIoOutput {
            ended_at: Utc::now(),
            output,
        });
    }
    pub fn standalone_query_start(&self, execution_id: Uuid, query: QueryWithParameters) -> u64 {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let id = execution_context
            .replay_data
            .database_recording
            .standalone_queries_count;
        execution_context
            .replay_data
            .database_recording
            .standalone_queries_count += 1;
        execution_context
            .replay_data
            .database_recording
            .standalone_queries
            .push(QueryWithResult {
                id,
                started_at: Utc::now(),
                query_with_parameters: query,
                result: None,
            });
        id
    }
    pub fn standalone_query_end(
        &self,
        execution_id: Uuid,
        query_id: u64,
        result: Result<serde_json::Value, Error>,
    ) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let query_mut = execution_context
            .replay_data
            .database_recording
            .standalone_queries
            .iter_mut()
            .find(|q| q.id == query_id)
            .unwrap();
        assert!(query_mut.result.is_none());
        query_mut.result = Some(QueryResult {
            ended_at: Utc::now(),
            result,
        })
    }

    pub fn transaction_start(&self, execution_id: Uuid) -> u64 {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let id = execution_context
            .replay_data
            .database_recording
            .transactions_count;
        execution_context
            .replay_data
            .database_recording
            .transactions_count += 1;
        execution_context
            .replay_data
            .database_recording
            .transactions
            .push(Transaction {
                id,
                started_at: Utc::now(),
                queries_count: 0,
                queries: vec![],
                result: None,
            });
        id
    }
    pub fn transaction_query_start(
        &self,
        execution_id: Uuid,
        transaction_id: u64,
        query: QueryWithParameters,
    ) -> u64 {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let transaction = execution_context
            .replay_data
            .database_recording
            .transactions
            .iter_mut()
            .find(|transaction| transaction.id == transaction_id)
            .unwrap();
        let id = transaction.queries_count;
        transaction.queries_count += 1;
        transaction.queries.push(QueryWithResult {
            id,
            started_at: Utc::now(),
            query_with_parameters: query,
            result: None,
        });
        id
    }
    pub fn transaction_query_end(
        &self,
        execution_id: Uuid,
        transaction_id: u64,
        query_id: u64,
        result: Result<serde_json::Value, Error>,
    ) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let transaction = execution_context
            .replay_data
            .database_recording
            .transactions
            .iter_mut()
            .find(|transaction| transaction.id == transaction_id)
            .unwrap();
        let query_result = transaction
            .queries
            .iter_mut()
            .find(|query| query.id == query_id)
            .unwrap();
        assert!(query_result.result.is_none());
        query_result.result = Some(QueryResult {
            ended_at: Utc::now(),
            result,
        });
    }

    pub fn transaction_end(
        &self,
        execution_id: Uuid,
        transaction_id: u64,
        result: Result<(), Error>,
    ) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let transaction = execution_context
            .replay_data
            .database_recording
            .transactions
            .iter_mut()
            .find(|transaction| transaction.id == transaction_id)
            .unwrap();
        assert!(transaction.result.is_none());
        transaction.result = Some(TransactionResult {
            ended_at: Utc::now(),
            result,
        });
    }
*/
