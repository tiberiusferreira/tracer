use crate::parameters::Parameter;
use crate::{Error, IoEvent, PgIoRecorderConnection, QueryRequest, QueryResult, QueryType, RECORDER_NAME, record_io_response_as_query_result, sqlx_error_to_recorder_error, Transaction, TransactionIoProvider};
use serde_json::json;


impl PgIoRecorderConnection {
    pub async fn execute(
        &mut self,
        query: &str,
        parameters: Vec<Parameter>,
    ) -> Result<(), Error> {
        let request_event = IoEvent::QueryRequest(QueryRequest {
            tx_id: None,
            query_text: query.to_string(),
            query_type: QueryType::Execute,
            parameters: parameters.clone(),
        });
        let client = match self {
            PgIoRecorderConnection::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event =
                    w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::QueryResult(QueryResult(result)) = recorded_response_event.value
                else {
                    panic!("unexpected response type")
                };
                let res = result?;
                assert_eq!(res, json!(null));
                return Ok(());
            }
            PgIoRecorderConnection::Live(client) => client,
        };
        let io_req_json = request_event.as_json();
        let recorded_io_req =
            tracer::recorder_api::record_io_event_request_or_panic(RECORDER_NAME, io_req_json);
        let raw_io_response: Result<(), Error> =
            raw_execute(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());
        raw_io_response
    }
}

impl<'a> Transaction<'a> {
    pub async fn execute(
        &mut self,
        query: &str,
        parameters: Vec<Parameter>,
    ) -> Result<(), Error> {
        let request_event = IoEvent::QueryRequest(QueryRequest {
            tx_id: Some(self.id),
            query_text: query.to_string(),
            query_type: QueryType::Execute,
            parameters: parameters.clone(),
        });
        let client = match &mut self.tx {
            TransactionIoProvider::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event =
                    w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::QueryResult(QueryResult(result)) = recorded_response_event.value
                else {
                    panic!("unexpected response type")
                };
                let res = result?;
                assert_eq!(res, json!(null));
                return Ok(());
            }
            TransactionIoProvider::Live(client) => client,
        };
        let io_req_json = request_event.as_json();
        let recorded_io_req =
            tracer::recorder_api::record_io_event_request_or_panic(RECORDER_NAME, io_req_json);
        let raw_io_response: Result<(), Error> =
            raw_execute(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());
        raw_io_response
    }
}


async fn raw_execute(
    con: &mut sqlx::PgConnection,
    query: &str,
    parameters: Vec<Parameter>,
) -> Result<(), Error> {
    let mut sqlx_query = sqlx::query(&query);
    for single_binding in &parameters {
        match single_binding {
            Parameter::String(p) => {
                sqlx_query = sqlx_query.bind(p);
            }
            Parameter::Date(p) => {
                sqlx_query = sqlx_query.bind(p);
            }
            Parameter::Datetime(p) => {
                sqlx_query = sqlx_query.bind(p);
            }
            Parameter::Bool(p) => {
                sqlx_query = sqlx_query.bind(p);
            }
            Parameter::Json(p) => {
                sqlx_query = sqlx_query.bind(p);
            }
            Parameter::I32(p) => {
                sqlx_query = sqlx_query.bind(p);
            }
            Parameter::I64(p) => {
                sqlx_query = sqlx_query.bind(p);
            }
            Parameter::I32Array(p) => {
                sqlx_query = sqlx_query.bind(p);
            }
        }
    }
    let _res = sqlx_query
        .execute(&mut *con)
        .await
        .map_err(|e| sqlx_error_to_recorder_error(e, &query, &parameters))?;
    Ok(())
}
