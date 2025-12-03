use std::panic::Location;
use tracer::recorder_api::record_io_event_request_or_panic;
use tracked_error::error_chain_to_pretty_formatted;
use crate::{Error, IoEvent, Transaction, TransactionIoProvider, TxCommitRequest, TxCommitResult, RECORDER_NAME};

impl<'a> Transaction<'a> {
    pub async fn commit(self) -> Result<(), Error> {
        let request_event = IoEvent::TxCommitRequest(TxCommitRequest { tx_id: self.id });
        let tx = match self.tx {
            TransactionIoProvider::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::TxCommitResult(TxCommitResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                return result;
            }
            TransactionIoProvider::Live(tx) => tx,
        };
        let recorded_io_req = record_io_event_request_or_panic(RECORDER_NAME, request_event);
        let raw_io_response = tx.commit().await.map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            Error::Internal {
                msg: err_str,
                location: Location::caller().to_string(),
            }
        });
        let is_err = raw_io_response.is_err();
        let io_response = IoEvent::TxCommitResult(TxCommitResult(raw_io_response.clone()));
        recorded_io_req.record_response_serializing_and_panicking(io_response, is_err);
        raw_io_response
    }
}
