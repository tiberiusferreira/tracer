use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing_config_helper::io_provider::execution_recorder::{
    get_current_execution, get_global_collector,
};

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
                let execution_id = get_current_execution().unwrap();
                let collector = get_global_collector();
                let request = IoEvent::CurrentDateRequest;
                let event_json = serde_json::to_value(&request).unwrap();
                let event_id =
                    collector.record_io_event(execution_id, RECORDER_NAME, event_json, false, None);

                let datetime = Utc::now();
                let response = IoEvent::CurrentDateResponse(datetime);
                let event_json = serde_json::to_value(&response).unwrap();
                collector.record_io_event(
                    execution_id,
                    RECORDER_NAME,
                    event_json,
                    false,
                    Some(event_id),
                );
                datetime
            }
        }
    }
}
