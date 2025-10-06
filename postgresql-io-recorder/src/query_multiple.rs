use std::panic::Location;
use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres};
use tracer::io_provider::record_io_event_request;
use crate::{record_io_response_as_query_result, sqlx_error_to_recorder_error, PgIoRecorder, Error, IoEvent, QueryRequest, QueryResult, QueryType, RECORDER_NAME};
use crate::parameters::Parameter;

impl PgIoRecorder {
    pub async fn query_multiple<
        IntoString: Into<String>,
        Out: Serialize + DeserializeOwned + Clone + Send + Unpin + for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row>,
    >(
        &self,
        query: &str,
        parameters: IndexMap<IntoString, Parameter>,
    ) -> Result<Vec<Out>, Error> {
        let parameters: IndexMap<String, Parameter> =
            parameters.into_iter().map(|(k, v)| (k.into(), v)).collect();
        let request_event = IoEvent::QueryRequest(QueryRequest {
            tx_id: None,
            query_text: query.to_string(),
            query_type: QueryType::Multiple,
            parameters: parameters.clone(),
        });
        let client = match &self {
            PgIoRecorder::Recorded(recording) => {
                let mut w_guard = recording.write().unwrap();
                let recorded_response_event = w_guard.get_io_event_response_marking_events_as_used(&request_event);
                let IoEvent::QueryResult(QueryResult(result)) = recorded_response_event.value else {
                    panic!("unexpected response type")
                };
                let res = result?;
                let res: Vec<Out> = serde_json::from_value(res).expect("result was not the correct type");
                return Ok(res);
            }
            PgIoRecorder::Live(client) => client,
        };
        let io_req_json = request_event.as_json();
        let recorded_io_req = record_io_event_request(RECORDER_NAME, io_req_json);
        let raw_io_response: Result<Vec<Out>, Error> = raw_query_multiple(client, query, parameters).await;
        record_io_response_as_query_result(recorded_io_req, raw_io_response.clone());
        raw_io_response
    }
}


async fn raw_query_multiple<T: Serialize + DeserializeOwned + Clone + Send + Unpin + for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row>>(con: &PgPool, query: &str, parameters: IndexMap<String, Parameter>) -> Result<Vec<T>, Error> {
    let original_query = query.to_string();
    let mut bindings = vec![];

    let replaced_query = crate::placeholder_replacements::replace_query(original_query.clone());

    for placeholder in &replaced_query.placeholders {
        let replacement = parameters.get(placeholder).ok_or_else(|| Error::Internal {
            msg: format!("Missing replacement for parameter {placeholder} in query {original_query}", placeholder = placeholder),
            location: Location::caller().to_string(),
        })?;
        bindings.push(replacement);
    }


    let mut query = sqlx::query_as(&replaced_query.replaced_query);
    for single_binding in bindings {
        match single_binding {
            Parameter::String(p) => {
                query = query.bind(p);
            }
            Parameter::Date(p) => {
                query = query.bind(p);
            }
            Parameter::Datetime(p) => {
                query = query.bind(p);
            }
            Parameter::Bool(p) => {
                query = query.bind(p);
            }
            Parameter::Json(p) => {
                query = query.bind(p);
            }
            Parameter::I32(p) => {
                query = query.bind(p);
            }
            Parameter::I64(p) => {
                query = query.bind(p);
            }
            Parameter::I32Array(p) => {
                query = query.bind(p);
            }
        }
    }
    let res: Vec<T> = query
        .fetch_all(con)
        .await
        .map_err(|e| sqlx_error_to_recorder_error(e, &replaced_query.replaced_query, &parameters))?;
    Ok(res)
}