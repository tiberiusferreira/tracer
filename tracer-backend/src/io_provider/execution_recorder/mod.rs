use crate::io_provider::execution_recorder::database::Query;
use crate::io_provider::execution_recorder::function_instrumentation::{
    ExecutingFunction, track_task,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{OnceLock, RwLock};
use uuid::Uuid;

pub mod database;
pub mod function_instrumentation;
pub static GLOBAL_DATA_COLLECTOR: OnceLock<DataCollector> = OnceLock::new();

pub async fn record_execution<
    F: Future,
    Input: Serialize + DeserializeOwned,
    Fun: FnOnce(Input) -> F,
>(
    name: &str,
    kind: ExecutionKind,
    input: Input,
    future_generator: Fun,
) -> <F as Future>::Output {
    let execution_context_id = get_global_collector().register_new_execution(name, kind);
    let future = future_generator(input);
    let res = track_task(future, execution_context_id).await;
    res
}

#[derive(Debug)]
pub struct DataCollector {
    executions: RwLock<HashMap<Uuid, ExecutionRecording>>,
}
impl DataCollector {
    pub fn new() -> Self {
        Self {
            executions: RwLock::new(HashMap::new()),
        }
    }
    pub fn get_all(&self) -> Vec<ExecutionRecording> {
        self.executions.read().unwrap().values().cloned().collect()
    }
    pub fn register_new_execution(&self, name: &str, kind: ExecutionKind) -> Uuid {
        let id = Uuid::new_v4();

        assert!(
            self.executions
                .write()
                .unwrap()
                .insert(id, ExecutionRecording::new(id, name, kind))
                .is_none(),
            "execution already registered"
        );
        id
    }
    pub fn end_execution(&self, id: Uuid) {
        self.executions
            .write()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .ended_at = Some(Utc::now());
    }

    pub fn standalone_query_start(&self, execution_id: Uuid, query: Query) -> u64 {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let id = execution_context
            .database_recording
            .standalone_queries
            .len() as u64;
        execution_context
            .database_recording
            .standalone_queries
            .push(QueryWithResult {
                started_at: Utc::now(),
                query,
                result: None,
            });
        id
    }
    pub fn standalone_query_end(
        &self,
        execution_id: Uuid,
        query_id: u64,
        result: Result<serde_json::Value, database::Error>,
    ) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let query_mut = execution_context
            .database_recording
            .standalone_queries
            .get_mut(query_id as usize)
            .unwrap();
        assert!(query_mut.result.is_none());
        query_mut.result = Some(QueryResult {
            result,
            ended_at: Utc::now(),
        });
    }

    pub fn transaction_start(&self, execution_id: Uuid) -> u64 {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let id = execution_context.database_recording.transactions.len() as u64;
        execution_context
            .database_recording
            .transactions
            .push(Transaction {
                started_at: Utc::now(),
                queries: vec![],
                result: None,
            });
        id
    }
    pub fn transaction_query_start(
        &self,
        execution_id: Uuid,
        transaction_id: u64,
        query: Query,
    ) -> u64 {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let transaction = execution_context
            .database_recording
            .transactions
            .get_mut(transaction_id as usize)
            .unwrap();
        let id = transaction.queries.len() as u64;
        transaction.queries.push(QueryWithResult {
            started_at: Utc::now(),
            query,
            result: None,
        });
        id
    }
    pub fn transaction_query_end(
        &self,
        execution_id: Uuid,
        transaction_id: u64,
        query_id: u64,
        result: Result<serde_json::Value, database::Error>,
    ) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let transaction = execution_context
            .database_recording
            .transactions
            .get_mut(transaction_id as usize)
            .unwrap();
        let query_result = transaction.queries.get_mut(query_id as usize).unwrap();
        assert!(query_result.result.is_none());
        query_result.result = Some(QueryResult {
            result,
            ended_at: Utc::now(),
        });
    }

    pub fn transaction_end(
        &self,
        execution_id: Uuid,
        transaction_id: u64,
        result: Result<(), database::Error>,
    ) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let transaction = execution_context
            .database_recording
            .transactions
            .get_mut(transaction_id as usize)
            .unwrap();
        assert!(transaction.result.is_none());
        transaction.result = Some(TransactionResult {
            ended_at: Utc::now(),
            result,
        });
    }
}

pub fn get_global_collector() -> &'static DataCollector {
    GLOBAL_DATA_COLLECTOR
        .get()
        .expect("collector to have been initialized")
}

#[derive(Clone, Debug)]
struct QueryWithResult {
    started_at: DateTime<Utc>,
    query: Query,
    result: Option<QueryResult>,
}

#[derive(Clone, Debug)]
struct QueryResult {
    result: Result<serde_json::Value, database::Error>,
    ended_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
struct Transaction {
    started_at: DateTime<Utc>,
    queries: Vec<QueryWithResult>,
    result: Option<TransactionResult>,
}

#[derive(Clone, Debug)]
enum CommitResult {
    Commited,
    RolledBack,
}

#[derive(Clone, Debug)]
struct TransactionResult {
    ended_at: DateTime<Utc>,
    result: Result<(), database::Error>,
}

#[derive(Clone, Debug, Default)]
struct DatabaseRecording {
    standalone_queries: Vec<QueryWithResult>,
    transactions: Vec<Transaction>,
}

#[derive(Clone, Debug)]
pub struct HttpHandler {
    endpoint: String,
    response_status_code: Option<i64>,
}
#[derive(Clone, Debug)]
pub enum ExecutionKind {
    HttpHandler(HttpHandler),
    Other,
}

#[derive(Clone, Debug)]
pub struct ExecutionRecording {
    id: Uuid,
    kind: ExecutionKind,
    name: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    warning_messages: Vec<String>,
    error_messages: Vec<String>,
    database_recording: DatabaseRecording,
    executed_functions: Vec<ExecutingFunction>,
    call_stack: Vec<u64>,
}

impl ExecutionRecording {
    pub fn new(id: Uuid, name: &str, kind: ExecutionKind) -> ExecutionRecording {
        Self {
            id,
            database_recording: DatabaseRecording::default(),
            started_at: Utc::now(),
            name: name.to_string(),
            warning_messages: Default::default(),
            error_messages: Default::default(),
            ended_at: None,
            executed_functions: vec![],
            call_stack: vec![],
            kind,
        }
    }
}

thread_local! {
    pub static CURRENT_EXECUTION: Cell<Option<Uuid>> = const { Cell::new(None) };
}

fn set_current_execution(id: Uuid) {
    let old = CURRENT_EXECUTION.replace(Some(id));
    assert!(
        old.is_none(),
        "there was already some execution with id {old:#?}"
    );
}
pub fn get_current_execution() -> Option<Uuid> {
    let curr = CURRENT_EXECUTION.get();
    curr
}

fn clear_current_execution(id: Uuid) {
    let old = CURRENT_EXECUTION.replace(None);
    assert_eq!(old, Some(id), "execution didnt match {old:#?} {id}");
}

#[derive(Debug, Clone)]
pub struct ExecutionMetadata {
    name: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    warning_messages: Vec<String>,
    error_messages: Vec<String>,
}

impl ExecutionMetadata {
    pub fn record_error(&mut self, error: &str) {
        self.error_messages.push(error.to_string());
    }
    pub fn record_warning(&mut self, warning: &str) {
        self.warning_messages.push(warning.to_string());
    }
}
