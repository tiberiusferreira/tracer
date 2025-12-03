use crate::recording::global_recorder::{get_current_execution, get_global_recorder};
use tracked_error::error_chain_to_pretty_formatted;
use uuid::Uuid;

pub mod global_recorder;

/// Represents an IO Event Request.
///
/// The request might have been exported already or still be in the recording buffer.
/// This is mainly used to easily record the response of the request.
#[derive(Debug)]
pub struct IoEventRequest {
    pub io_provider_name: &'static str,
    pub execution_id: Uuid,
    pub event_id: Uuid,
}

/// Records an IO Event in the internal buffer.
///
/// The event can be exported at any point after this.
///
/// Returns an [IoEventRequest] which can be used to record the response.
///
/// # Panics
///
/// This function panics if provided an invalid `execution_id`.
fn record_io_event_request(
    execution_id: Uuid,
    io_provider_name: &'static str,
    event: serde_json::Value,
) -> IoEventRequest {
    let event_id =
        get_global_recorder().record_io_event(execution_id, io_provider_name, event, false, None);
    IoEventRequest {
        io_provider_name,
        execution_id,
        event_id,
    }
}

/// Records an IO Event in the internal buffer.
///
/// The event can be exported at any point after this.
///
/// Returns an [IoEventRequest] which can be used to record the response.
///
/// # Panics
///
/// This function panics if the serialization panics or if called outside an execution recording scope.
pub fn record_io_event_request_or_panic<T: serde::Serialize>(
    io_provider_name: &'static str,
    event: T,
) -> IoEventRequest {
    let event = serde_json::to_value(event).unwrap_or_else(|e| {
        let e = error_chain_to_pretty_formatted(&e);
        panic!("failed to serialize event from {io_provider_name}: {e}")
    });
    let execution_id = get_current_execution().unwrap_or_else(|| panic!("tried to record an io event for {io_provider_name} outside an execution recording scope"));
    record_io_event_request(execution_id, io_provider_name, event)
}

impl IoEventRequest {
    pub fn record_response(self, response: serde_json::Value, is_error: bool) {
        let execution_id = get_current_execution().unwrap();
        assert_eq!(
            execution_id, self.execution_id,
            "tried to record an io event response in a different execution than the on where the request was created"
        );
        get_global_recorder().record_io_event(
            execution_id,
            self.io_provider_name,
            response,
            is_error,
            Some(self.event_id),
        );
    }

    /// Records the response event of the IO Request Event in the internal buffer.
    ///
    /// The event can be exported at any point after this.
    ///
    /// # Panics
    ///
    /// This function panics if the serialization panics or if called outside an execution recording scope.
    pub fn record_response_serializing_and_panicking<T: serde::Serialize>(
        self,
        response: T,
        is_error: bool,
    ) {
        let event = serde_json::to_value(response).unwrap_or_else(|e| {
            let e = error_chain_to_pretty_formatted(&e);
            panic!("failed to serialize response event from {self:#?}: {e}")
        });
        self.record_response(event, is_error);
    }
}
