use crate::io_provider::TransactionIoProvider;
use crate::io_provider::execution_recorder::function_instrumentation::track_task;
use api_structs::instance::update::{
    Error, ExecutionRecording, QueryResult, QueryWithParameters, QueryWithResult, Transaction,
    TransactionResult,
};
use chrono::{DateTime, Utc};
use pin_project_lite::pin_project;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::pin::Pin;
use std::sync::{OnceLock, RwLock};
use std::task::{Context, Poll};
use uuid::Uuid;

pub mod function_instrumentation;
pub static GLOBAL_DATA_COLLECTOR: OnceLock<DataCollector> = OnceLock::new();

pub async fn record_execution<
    F: Future,
    Input: Serialize + DeserializeOwned,
    Fun: FnOnce(Input) -> F,
>(
    input: Input,
    future_generator: Fun,
    recording_enabled: bool,
) -> <F as Future>::Output {
    let input_json = serde_json::to_value(&input).unwrap();
    let execution_context_id =
        get_global_collector().register_new_execution(input_json, recording_enabled);
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
    pub fn get_all_pruning(&self) -> Vec<ExecutionRecording> {
        let data: Vec<ExecutionRecording> = self
            .executions
            .read()
            .unwrap()
            .values()
            .filter(|v| v.recording_enabled)
            .cloned()
            .collect();
        let mut w_guard = self.executions.write().unwrap();
        w_guard.retain(|k, val| !val.ended);
        data
    }
    pub fn register_new_execution(
        &self,
        input: serde_json::Value,
        recording_enabled: bool,
    ) -> Uuid {
        let id = Uuid::new_v4();

        assert!(
            self.executions
                .write()
                .unwrap()
                .insert(id, ExecutionRecording::new(id, input, recording_enabled))
                .is_none(),
            "execution already registered"
        );
        id
    }
    pub fn end_execution(&self, id: Uuid) {
        self.executions.write().unwrap().get_mut(&id).unwrap().ended = true;
        self.executions
            .write()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .last_seen_at = Utc::now();
    }

    pub fn standalone_query_start(&self, execution_id: Uuid, query: QueryWithParameters) -> u64 {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let is_recording = get_recording_database_state();
        if !is_recording {
            return 0;
        }
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
        let is_recording = get_recording_database_state();
        if !is_recording {
            return;
        }
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
        let is_recording = get_recording_database_state();
        if !is_recording {
            return 0;
        }
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
        let is_recording = get_recording_database_state();
        if !is_recording {
            return;
        }
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
    pub fn record_single_attribute(&self, execution_id: Uuid, name: String, value: String) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        execution_context
            .attributes
            .insert(name, HashSet::from([value]));
    }
}

pub fn record_single_attribute(name: String, value: String) {
    let current_exec = get_current_execution().unwrap();
    get_global_collector().record_single_attribute(current_exec, name, value);
}

pub fn get_global_collector() -> &'static DataCollector {
    GLOBAL_DATA_COLLECTOR
        .get()
        .expect("collector to have been initialized")
}

pin_project! {
    pub struct NoDatabaseRecording<F> {
        #[pin]
        inner: F,
        is_playing_a_recording: bool,
    }
    //  impl<T> PinnedDrop for Fut<T> {
    //     fn drop(this: Pin<&mut Self>) {
    //         let this = this.project();
    //         if let Some(function_id) = *this.function_id {
    //             let Some(current_execution) = get_current_execution() else {
    //                 panic!("tried to end function {function_id} without execution context");
    //             };
    //             get_global_collector().end_function(current_execution, function_id);
    //         }
    //     }
    // }
}
impl<E, T: Future<Output = Result<(), E>>> Future for NoDatabaseRecording<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let fut = this.inner;
        if *this.is_playing_a_recording {
            return Poll::Ready(Ok(()));
        }
        set_recording_database_state(false);
        let res = fut.poll(cx);
        set_recording_database_state(true);
        res
    }
}
pub fn run_without_query_recording<F: Future>(
    state: &crate::io_provider::IoProviderState,
    fut: F,
) -> NoDatabaseRecording<F> {
    let is_playing_a_recording = state.is_playing_recording;
    NoDatabaseRecording {
        inner: fut,
        is_playing_a_recording,
    }
}

thread_local! {
    pub static CURRENT_EXECUTION: Cell<Option<Uuid>> = const { Cell::new(None) };
    pub static RECORDING_DATABASE: Cell<bool> = const { Cell::new(true) };
}

fn get_recording_database_state() -> bool {
    RECORDING_DATABASE.get()
}
fn set_recording_database_state(new_state: bool) {
    RECORDING_DATABASE.replace(new_state);
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
