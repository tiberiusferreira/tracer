use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tracer::io_provider::{record_io_event_request, EventRecordingPlayhead};

pub const RECORDER_NAME: &str = "Datetime";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum IoEvent {
    CurrentDateRequest,
    CurrentDateResponse(DateTime<Utc>),
}


#[derive(Clone)]
pub enum CurrentDatetimeIoRecorder {
    Recording(Arc<RwLock<EventRecordingPlayhead<IoEvent>>>),
    Live,
}

impl CurrentDatetimeIoRecorder {
    pub fn from_global_recording() -> Self {
        let io_events = tracer::io_provider::get_io_provider_recorded_events(RECORDER_NAME)
            .unwrap_or_else(|| panic!("{RECORDER_NAME} to have recording"));
        let io_events: Vec<tracer::SpecializedIoEvent<IoEvent>> = tracer::io_provider::specialize_events_or_panic(io_events);

        Self::Recording(Arc::new(RwLock::new(EventRecordingPlayhead {
            events: io_events,
            used_events: Default::default(),
        })))
    }
    pub fn get_current_datetime(&self) -> DateTime<Utc> {
        match self {
            CurrentDatetimeIoRecorder::Recording(recording) => {
                let request_event = IoEvent::CurrentDateRequest;
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::CurrentDateResponse(resp) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                resp
            }
            CurrentDatetimeIoRecorder::Live => {
                let request = IoEvent::CurrentDateRequest;
                let event_json = serde_json::to_value(&request).unwrap();
                let io_request = record_io_event_request(RECORDER_NAME, event_json);
                let datetime = Utc::now();
                let response = IoEvent::CurrentDateResponse(datetime);
                let event_json = serde_json::to_value(&response).unwrap();
                io_request.record_response(event_json, false);
                datetime
            }
        }
    }
}
