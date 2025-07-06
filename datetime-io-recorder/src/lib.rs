use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing_config_helper::io_provider::{record_io_event_request};

pub const RECORDER_NAME: &str = "Datetime";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum IoEvent {
    CurrentDateRequest,
    CurrentDateResponse(DateTime<Utc>),
}

#[derive(Clone)]
pub struct RecordingBeingPlayed {
    pub events: Vec<IoEvent>,
    pub played_events: HashSet<usize>,
}
#[derive(Clone)]
pub enum CurrentDatetimeIoRecorder {
    Recorded(Arc<RwLock<RecordingBeingPlayed>>),
    Live,
}

impl CurrentDatetimeIoRecorder {
    pub fn get_current_datetime(&self) -> DateTime<Utc> {
        match self {
            CurrentDatetimeIoRecorder::Recorded(_recording) => {
                // let mut w_guard = recording.write().unwrap();
                // let (datetime, idx) = w_guard
                //     .events
                //     .iter()
                //     .enumerate()
                //     .find_map(|(idx, e)| match e {
                //         IoEvent::CurrentDateRequest(datetime) => {
                //             if w_guard.played_events.contains(&idx) {
                //                 None
                //             } else {
                //                 Some((*datetime, idx))
                //             }
                //         } // IoEvent::CurrentLocalTimezone(_) => None,
                //     })
                //     .unwrap();
                // w_guard.played_events.insert(idx);
                // datetime
                unimplemented!()
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
