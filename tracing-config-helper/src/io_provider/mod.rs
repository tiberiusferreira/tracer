use std::collections::HashSet;
use crate::io_provider::execution_recorder::{get_current_execution, get_global_collector};
use uuid::Uuid;
use api_structs::instance::update::{ExecutionRecordingSnapshot, IoEvent, SpecializedIoEvent};
use std::fs;

pub mod execution_recorder;


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
        let provider_events = rec.replay_data_fragment.io_providers_events.get(io_provider_name).cloned().unwrap_or_default();
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


pub fn is_playing_recording() -> bool {
    std::env::var("GLOBAL_RECORDING_PATH".to_string()).is_ok()
}
pub fn get_current_recording_input() -> Option<serde_json::Value> {
    let global_recording_path = std::env::var("GLOBAL_RECORDING_PATH".to_string()).ok()?;
    let recording_fragments = load_all_recordings_from_dir(&global_recording_path);
    let mut curr_input = None;
    for r in recording_fragments {
        if let Some(input) = r.replay_data_fragment.input {
            assert!(curr_input.is_none(), "found multiple inputs in the global recording");
            curr_input = Some(input);
        }
    }
    curr_input
}
pub fn get_io_provider_recording_events(io_provider_name: &'static str) -> Option<Vec<IoEvent>> {
    // if no global recording, we are live
    let global_recording_path = std::env::var("GLOBAL_RECORDING_PATH".to_string()).ok()?;

    // no overwrite, use global
    let recording_fragments = load_all_recordings_from_dir(&global_recording_path);
    Some(get_all_events_for_io_recorder(&recording_fragments, io_provider_name))
}

#[derive(Debug)]
pub struct IoEventRequest {
    pub io_provider_name: &'static str,
    pub execution_id: Uuid,
    pub event_id: Uuid,
}

pub fn record_io_event_request(
    io_provider_name: &'static str,
    event: serde_json::Value,
) -> IoEventRequest {
    let execution_id = get_current_execution().unwrap();
    let event_id =
        get_global_collector().record_io_event(execution_id, io_provider_name, event, false, None);
    IoEventRequest {
        io_provider_name,
        execution_id,
        event_id,
    }
}


pub fn record_io_event_request_serializing_and_panicking<T: serde::Serialize>(
    io_provider_name: &'static str,
    event: T,
) -> IoEventRequest {
    let event = serde_json::to_value(event).unwrap();
    record_io_event_request(io_provider_name, event)
}

impl IoEventRequest {
    pub fn record_response(self, response: serde_json::Value, is_error: bool) {
        let execution_id = get_current_execution().unwrap();
        assert_eq!(
            execution_id, self.execution_id,
            "tried to record an io event response in a different execution than the on where the request was created"
        );
        get_global_collector().record_io_event(
            execution_id,
            self.io_provider_name,
            response,
            is_error,
            Some(self.event_id),
        );
    }
    pub fn record_response_serializing_and_panicking<T: serde::Serialize>(self, response: T, is_error: bool) {
        let event = serde_json::to_value(response).unwrap();
        self.record_response(event, is_error);
    }
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