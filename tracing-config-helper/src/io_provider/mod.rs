use crate::io_provider::execution_recorder::{get_current_execution, get_global_collector};
use api_structs::instance::update::{Error, Parameter, QueryWithParameters};
use gel_protocol::value_opt::ValueOpt;
use gel_tokio::RawTransaction;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracked_error::error_chain_to_pretty_formatted;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Id {
    pub id: uuid::Uuid,
}
pub mod example_usage;
pub mod execution_recorder;
mod gel;

#[derive(Clone)]
pub struct ExecutionIoProvider {
    pub database: DatabaseIoProvider,
}

impl ExecutionIoProvider {
    pub fn database(&self) -> &DatabaseIoProvider {
        &self.database
    }
}

#[derive(Clone)]
pub enum DatabaseIoProvider {
    Recorded(()),
    Live(gel_tokio::Client),
}

pub enum TransactionIoProvider {
    Recorded(()),
    Live(RawTransaction),
}

pub struct Transaction {
    id: u64,
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
            }
        }
        let new = serde_json::Value::Object(new);
        // let new = Value::Json(gel_protocol::model::Json::new_unchecked(
        //     serde_json::to_string_pretty(&new).unwrap(),
        // ));
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
        let execution_id = get_current_execution().unwrap();
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(_) => {
                unimplemented!()
            }
            TransactionIoProvider::Live(tx) => tx,
        };

        let global_collector = get_global_collector();
        let query_id = global_collector.transaction_query_start(
            execution_id,
            self.id,
            QueryWithParameters {
                query_text: query.to_string(),
                parameters: parameters.clone(),
            },
        );
        let result: Result<T, Error> = run_tx_query_required(client, query, parameters).await;
        let res_as_json_value = result.clone().map(|v| serde_json::to_value(v).unwrap());
        global_collector.transaction_query_end(execution_id, self.id, query_id, res_as_json_value);
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
        let query_id = global_collector.transaction_query_start(
            execution_id,
            self.id,
            QueryWithParameters {
                query_text: query.to_string(),
                parameters: parameters.clone(),
            },
        );
        let result: Result<Option<T>, Error> =
            run_tx_query_optional(client, query, parameters).await;
        let res_as_json_value = result.clone().map(|v| serde_json::to_value(v).unwrap());
        global_collector.transaction_query_end(execution_id, self.id, query_id, res_as_json_value);
        result
    }
    pub async fn commit(self) -> Result<(), Error> {
        let execution_id = get_current_execution().unwrap();
        let global_collector = get_global_collector();
        let mut tx = match self.tx {
            TransactionIoProvider::Recorded(_) => {
                unimplemented!()
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let res = tx.commit().await.map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            Error::Internal {
                msg: err_str,
                location: std::panic::Location::caller().to_string(),
            }
        });

        global_collector.transaction_end(execution_id, self.id, res.clone());
        res
    }
}
impl DatabaseIoProvider {
    #[track_caller]
    pub async fn transaction_start(&self) -> Transaction {
        let execution_id = get_current_execution().unwrap();
        let client = match &self {
            DatabaseIoProvider::Recorded(_) => {
                unimplemented!()
            }
            DatabaseIoProvider::Live(client) => client,
        };
        let global_collector = get_global_collector();
        let tx = client.transaction_raw().await.unwrap();
        let tx_id = global_collector.transaction_start(execution_id);
        Transaction {
            id: tx_id,
            tx: TransactionIoProvider::Live(tx),
        }
    }

    #[track_caller]
    pub async fn query_required_single<T: Serialize + DeserializeOwned + Clone>(
        &self,
        query: &str,
        parameters: HashMap<String, Parameter>,
    ) -> Result<T, Error> {
        let execution_id = get_current_execution().unwrap();
        let client = match &self {
            DatabaseIoProvider::Recorded(_) => {
                unimplemented!()
            }
            DatabaseIoProvider::Live(client) => client,
        };
        let global_collector = get_global_collector();
        let query_id = global_collector.standalone_query_start(
            execution_id,
            QueryWithParameters {
                query_text: query.to_string(),
                parameters: parameters.clone(),
            },
        );
        let result: Result<T, Error> = run_query(client, query, parameters).await;
        let res_as_json_value = result.clone().map(|v| serde_json::to_value(v).unwrap());
        global_collector.standalone_query_end(execution_id, query_id, res_as_json_value);
        result
    }
}

#[track_caller]
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
        .unwrap_or_else(|_e| panic!("failed to deserialize query from {query:#?})"));
    Ok(query_result)
}

#[track_caller]
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
        .unwrap_or_else(|_e| panic!("failed to deserialize query from {query:#?})"));
    Ok(query_result)
}

#[track_caller]
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
        .unwrap_or_else(|_e| panic!("failed to deserialize query from {query:#?})"));
    Ok(Some(query_result))
}
