use crate::io_provider::execution_recorder::{get_current_execution, get_global_collector};
use uuid::Uuid;

pub mod execution_recorder;

#[derive(Debug)]
pub struct IoEventRequest {
    io_provider_name: &'static str,
    execution_id: Uuid,
    event_id: Uuid,
}
pub fn record_io_event_request(
    io_provider_name: &'static str,
    event: serde_json::Value,
) -> IoEventRequest {
    if let Some(execution_id) = get_current_execution() {
        let event_id = get_global_collector().record_io_event(
            execution_id,
            io_provider_name,
            event,
            false,
            None,
        );
        IoEventRequest {
            io_provider_name,
            execution_id,
            event_id,
        }
    } else {
        IoEventRequest {
            io_provider_name,
            execution_id: Uuid::new_v4(),
            event_id: Uuid::new_v4(),
        }
    }
}

impl IoEventRequest {
    pub fn record_response(self, response: serde_json::Value, is_error: bool) {
        if let Some(execution_id) = get_current_execution() {
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
    }
}
