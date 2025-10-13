use std::collections::HashSet;
use std::fs;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::recording::global_recorder::{ExecutionRecordingSnapshot, IoEvent};

pub fn get_io_provider_recorded_events(io_provider_name: &'static str) -> Option<Vec<IoEvent>> {
    // if no global recording, we are live
    let global_recording_path = std::env::var("GLOBAL_RECORDING_PATH".to_string()).ok()?;

    // no overwrite, use global
    let recording_fragments = load_all_recordings_from_dir(&global_recording_path);
    Some(get_all_events_for_io_recorder(&recording_fragments, io_provider_name))
}


pub fn get_current_recording_input() -> Option<serde_json::Value> {
    let global_recording_path = std::env::var("GLOBAL_RECORDING_PATH".to_string()).ok()?;
    let recording_fragments = load_all_recordings_from_dir(&global_recording_path);
    let mut curr_input = None;
    for r in recording_fragments {
        if let Some(input) = r.execution_io_fragment.input {
            assert!(curr_input.is_none(), "found multiple inputs in the global recording");
            curr_input = Some(input);
        }
    }
    curr_input
}
fn load_all_recordings_from_dir(dir: &str) -> Vec<ExecutionRecordingSnapshot> {
    let json_entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("failed to read recording directory {dir} {e:?}"))
        .filter_map(|entry| {
            let entry = entry.as_ref().expect("failed to read entry").path();
            let is_json = entry
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "json")
                .unwrap_or(false);
            if is_json {
                Some(entry)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut recordings = vec![];
    for e in json_entries {
        recordings.push(load_recording_fragment(e.to_str().unwrap().to_string()));
    }
    recordings
}

fn load_recording_fragment(recording_path: String) -> ExecutionRecordingSnapshot {
    let file_content = std::fs::read_to_string(&recording_path).unwrap_or_else(|e| panic!("recording file to exist at {recording_path}. {e:?}"));
    let recording: ExecutionRecordingSnapshot = serde_json::from_str(&file_content).expect("invalid recording file");
    recording
}

fn get_all_events_for_io_recorder(recording: &[ExecutionRecordingSnapshot], io_provider_name: &str) -> Vec<IoEvent> {
    let mut events = vec![];
    for rec in recording {
        let provider_events = rec.execution_io_fragment.io_providers_events.get(io_provider_name).cloned().unwrap_or_default();
        events.extend(provider_events);
    }
    events
}

pub fn specialize_events_or_panic<T: serde::de::DeserializeOwned>(events: Vec<IoEvent>) -> Vec<SpecializedIoEvent<T>> {
    let mut specialized_events: Vec<SpecializedIoEvent<T>> = vec![];
    for e in events {
        let value: T = serde_json::from_value(e.value).unwrap();
        specialized_events.push(SpecializedIoEvent {
            id: e.id,
            created_at: e.created_at,
            is_response_of: e.is_response_of,
            is_error: e.is_error,
            value,
        });
    }
    specialized_events
}

pub async fn play_global_recording<
    Output,
    Input: Serialize + DeserializeOwned,
    Fun: AsyncFnOnce(Input) -> Output,
>(
    future_generator: Fun,
) -> Output {
    let global_recording_path = std::env::var("GLOBAL_RECORDING_PATH".to_string()).ok().unwrap();
    let recording_fragments = load_all_recordings_from_dir(&global_recording_path);
    let recording_id = recording_fragments[0].id;
    crate::recording::global_recorder::set_current_execution(recording_id);
    let input = get_current_recording_input().expect("no recording input");
    let input: Input = serde_json::from_value(input).expect("input to match expected type");
    let future = future_generator(input).await;
    future
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpecializedIoEvent<T> {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub is_response_of: Option<Uuid>,
    pub is_error: bool,
    pub value: T,
}


pub struct EventRecordingPlayhead<T> {
    pub events: Vec<SpecializedIoEvent<T>>,
    pub used_events: HashSet<Uuid>,
}

impl<T: Clone + PartialEq + std::fmt::Debug> EventRecordingPlayhead<T> {
    pub fn find_first_unused(&self, io: &T) -> &SpecializedIoEvent<T> {
        let event = self.events.iter().find(|e| &e.value == io && !self.used_events.contains(&e.id)).unwrap_or_else(|| panic!("event not found: {io:#?}"));
        event
    }
    pub fn find_unused_response_of(&self, io_event_id: Uuid) -> &SpecializedIoEvent<T> {
        let event = self.events.iter().find(|e| e.is_response_of == Some(io_event_id) && !self.used_events.contains(&e.id)).expect("event response not found");
        event
    }
    pub fn get_io_event_response_marking_events_as_used(&mut self, io: &T) -> SpecializedIoEvent<T> {
        let request_id = self.find_first_unused(io).id;
        self.used_events.insert(request_id);
        let response = self.find_unused_response_of(request_id).clone();
        self.used_events.insert(response.id);
        response
    }
}