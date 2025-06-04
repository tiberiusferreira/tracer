use crate::io_provider::execution_recorder::function_instrumentation::track_task;
use api_structs::instance::update::{ExecutionRecording, IoEvent};
use chrono::Utc;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::{OnceLock, RwLock};
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

pub async fn play_execution<
    F: Future,
    Input: Serialize + DeserializeOwned,
    Fun: FnOnce(Input) -> F,
>(
    input: Input,
    future_generator: Fun,
) -> <F as Future>::Output {
    let future = future_generator(input).await;
    future
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
        let mut w_guard = self.executions.write().unwrap();
        let execution = w_guard.get_mut(&id).unwrap();
        assert!(!execution.ended);
        execution.ended = true;
        execution.last_seen_at = Utc::now();
    }

    pub fn record_io_event(
        &self,
        execution_id: Uuid,
        io_provider_name: &str,
        event: serde_json::Value,
        is_error: bool,
        is_response_of: Option<Uuid>,
    ) -> Uuid {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution = exec_context_w_guard.get_mut(&execution_id).unwrap();
        assert!(!execution.ended);
        let recorded_ios = execution
            .replay_data
            .io_providers_events
            .entry(io_provider_name.to_owned())
            .or_default();
        let event_id = Uuid::new_v4();
        recorded_ios.push(IoEvent {
            id: event_id,
            created_at: Utc::now(),
            is_response_of,
            is_error,
            value: event,
        });
        event_id
    }

    pub fn record_single_attribute(&self, execution_id: Uuid, name: String, value: String) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution = exec_context_w_guard.get_mut(&execution_id).unwrap();
        assert!(!execution.ended);
        execution.attributes.insert(name, HashSet::from([value]));
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
        w_guard.retain(|_k, val| !val.ended);
        if !w_guard.is_empty() {
            println!("{} executions still running", w_guard.len());
        }
        data
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
