use crate::recording::global_recorder::execution_tracking::track_task;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::{LazyLock, RwLock};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use tracing::debug;
use uuid::Uuid;
use tracked_error::error_chain_to_pretty_formatted;
use crate::is_playing_recording;

pub mod execution_tracking;
static GLOBAL_RECORDER: LazyLock<Box<dyn GlobalRecorder>> = LazyLock::new(|| {
    Box::new(Recorder::new())
});

pub type IoRecorderName = String;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IoEvent {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub is_response_of: Option<Uuid>,
    pub is_error: bool,
    pub value: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionIOFragment {
    pub input: Option<serde_json::Value>,
    pub io_providers_events: HashMap<IoRecorderName, Vec<IoEvent>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionRecordingSnapshot {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ended: bool,
    pub execution_io_fragment: ExecutionIOFragment,
    pub attributes: IndexMap<String, HashSet<String>>,
}

impl ExecutionRecordingSnapshot {
    pub fn new(id: Uuid, input: serde_json::Value) -> Self {
        Self {
            id,
            started_at: Utc::now(),
            last_seen_at: Utc::now(),
            ended: false,
            execution_io_fragment: ExecutionIOFragment {
                input: Some(input),
                io_providers_events: HashMap::new(),
            },
            attributes: IndexMap::new(),
        }
    }
}

pub trait GlobalRecorder: Send + Sync + Debug {
    fn record_execution_start(&self, input: serde_json::Value, drop_before_export: bool) -> Uuid;
    fn record_execution_end(&self, id: Uuid);

    fn record_io_event(
        &self,
        execution_id: Uuid,
        io_provider_name: &str,
        event: serde_json::Value,
        is_error: bool,
        is_response_of: Option<Uuid>,
    ) -> Uuid;

    fn record_attribute(&self, execution_id: Uuid, name: String, value: String);
    fn get_all_pruning(&self) -> Vec<ExecutionRecordingSnapshot>;
}

/// For now this is a concrete type, but it could be a trait in the future to allow external implementations.
pub fn get_global_recorder() -> &'static dyn GlobalRecorder {
    &**GLOBAL_RECORDER
}

pub async fn record_execution<
    FutureInput: Serialize + DeserializeOwned,
    FutureOutput,
    Future: AsyncFnOnce(FutureInput) -> FutureOutput,
>(
    input: FutureInput,
    future_generator: Future,
    drop_before_export: bool,
) -> FutureOutput {
    let input_json = serde_json::to_value(&input).expect("failed to serialize input");
    // the end is recorded when the future from `track_task` is dropped
    let execution_context_id =
        get_global_recorder().record_execution_start(input_json, drop_before_export);
    let future = future_generator(input);
    let res = track_task(future, execution_context_id).await;
    res
}


pub async fn record_execution_simple<
    Fut: Future<Output=()>,
>(
    fut: Fut,
    drop_before_export: bool,
) {
    record_execution((), async |()| { fut.await }, drop_before_export).await
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ExecutionToExportOrDrop {
    exec_rec: ExecutionRecordingSnapshot,
    drop_before_export: bool,
}

#[derive(Debug)]
pub struct Recorder {
    executions: RwLock<HashMap<Uuid, ExecutionToExportOrDrop>>,
}

impl GlobalRecorder for Recorder {
    fn record_execution_start(&self, input: serde_json::Value, drop_before_export: bool) -> Uuid {
        self.record_execution_start(input, drop_before_export)
    }

    fn record_execution_end(&self, id: Uuid) {
        self.record_execution_end(id);
    }

    fn record_io_event(&self, execution_id: Uuid, io_provider_name: &str, event: serde_json::Value, is_error: bool, is_response_of: Option<Uuid>) -> Uuid {
        if is_playing_recording() {
            panic!("recording during playback?");
        }
        self.record_io_event(execution_id, io_provider_name, event, is_error, is_response_of)
    }

    fn record_attribute(&self, execution_id: Uuid, name: String, value: String) {
        if is_playing_recording() {
            return;
        }
        self.record_attribute(execution_id, name, value);
    }

    fn get_all_pruning(&self) -> Vec<ExecutionRecordingSnapshot> {
        self.get_all_pruning()
    }
}


impl Recorder {
    fn new() -> Self {
        Self {
            executions: RwLock::new(HashMap::new()),
        }
    }

    fn record_execution_start(&self, input: serde_json::Value, drop_before_export: bool) -> Uuid {
        let id = Uuid::new_v4();
        assert!(
            self.executions
                .write()
                .unwrap()
                .insert(id, ExecutionToExportOrDrop {
                    exec_rec: ExecutionRecordingSnapshot::new(id, input),
                    drop_before_export,
                })
                .is_none(),
            "execution already registered"
        );
        id
    }
    fn record_execution_end(&self, id: Uuid) {
        let mut w_guard = self.executions.write().unwrap();
        let execution = &mut w_guard.get_mut(&id).unwrap().exec_rec;
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
        let execution = &mut exec_context_w_guard.get_mut(&execution_id).unwrap().exec_rec;
        assert!(!execution.ended);
        let recorded_ios = execution.execution_io_fragment
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

    fn record_attribute(&self, execution_id: Uuid, name: String, value: String) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution = &mut exec_context_w_guard.get_mut(&execution_id).unwrap().exec_rec;
        assert!(!execution.ended);
        execution.attributes.insert(name, HashSet::from([value]));
    }
    pub fn get_all_pruning(&self) -> Vec<ExecutionRecordingSnapshot> {
        let mut w_guard = self.executions.write().unwrap();
        for exec in w_guard.values_mut() {
            if !exec.exec_rec.ended {
                exec.exec_rec.last_seen_at = Utc::now();
            }
        }
        let data: Vec<ExecutionRecordingSnapshot> = w_guard
            .values()
            .filter_map(|v| if !v.drop_before_export {
                Some(&v.exec_rec)
            } else {
                None
            })
            .cloned()
            .collect();
        w_guard.retain(|_k, val| !val.exec_rec.ended);
        if !w_guard.is_empty() {
            debug!("{} executions still running", w_guard.len());
        }
        for exec in w_guard.values_mut() {
            exec.exec_rec.execution_io_fragment.input = None;
            exec.exec_rec.execution_io_fragment.io_providers_events.clear();
            exec.exec_rec.attributes.clear();
        }
        data
    }
}

pub fn record_attribute(name: String, value: String) {
    let current_exec = get_current_execution().unwrap_or_else(|| panic!("tried to record attribute {name} {value} outside of execution"));
    get_global_recorder().record_attribute(current_exec, name, value);
}

pub fn record_error<E: std::error::Error>(error: E) -> String {
    let error = error_chain_to_pretty_formatted(error);
    record_error_str(error.clone());
    let current_exec = get_current_execution().unwrap_or_else(|| panic!("tried to record error {error} outside of execution"));
    get_global_recorder().record_attribute(current_exec, "error".to_string(), error.clone());
    error
}

pub fn record_error_str(error: String) {
    let current_exec = get_current_execution().unwrap_or_else(|| panic!("tried to record error {error} outside of execution"));
    get_global_recorder().record_attribute(current_exec, "error".to_string(), error.clone());
}

thread_local! {
    pub static CURRENT_EXECUTION: Cell<Option<Uuid>> = const { Cell::new(None) };
}

pub fn set_current_execution(id: Uuid) {
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
