use std::panic::Location;
use std::sync::Arc;
use sqlx::Acquire;
use uuid::Uuid;
use tracer::recorder_api::record_io_event_request_or_panic;
use tracked_error::error_chain_to_pretty_formatted;
use crate::{Error, IoEvent, PgIoRecorderConnection, Transaction, TransactionIoProvider, TxStartResult, RECORDER_NAME};


impl PgIoRecorderConnection {
    pub async fn transaction_start(&mut self) -> Result<Transaction<'_>, Error> {
        let request_event = IoEvent::TxStartRequest;
        let client = match self {
            Self::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::TxStartResult(TxStartResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                let tx_id = result?;
                return Ok(Transaction {
                    id: tx_id,
                    tx: TransactionIoProvider::Recorded(Arc::clone(&recording)),
                });
            }
            Self::Live(client) => client,
        };
        let recorded_io_req = record_io_event_request_or_panic(RECORDER_NAME, request_event);
        let tx = client.begin().await.map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            let err_str = format!("{err_str} start tx");
            Error::Internal {
                msg: err_str,
                location: Location::caller().to_string(),
            }
        });
        let tx_id = Uuid::new_v4();
        let result = if let Err(e) = &tx {
            Err(e.clone())
        } else {
            Ok(tx_id)
        };
        let is_error = result.is_err();
        let raw_io_response = IoEvent::TxStartResult(TxStartResult(result));
        recorded_io_req.record_response_serializing_and_panicking(raw_io_response, is_error);
        Ok(Transaction {
            id: tx_id,
            tx: TransactionIoProvider::Live(tx?),
        })
    }
}
