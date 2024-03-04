use crate::api::handlers::ui::trace::{RawDbEvent, RawDbSpan};
use crate::api::state::AppState;
use crate::api::{handlers, u64_nanos_to_db_i64, ApiError};
use api_structs::ui::trace::chunk::{Event, SingleChunkTraceQuery, Span, TraceId};
use api_structs::Severity;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use backtraced_error::SqlxError;
use futures::TryFutureExt;
use sqlx::PgPool;
use std::collections::HashMap;
use std::io::Read;
use std::str::FromStr;
use tracing::{info, instrument};

#[instrument(skip_all, fields(trace_id=trace_id.trace_id))]
pub async fn get_trace_timestamp_chunks(
    con: &PgPool,
    trace_id: TraceId,
) -> Result<Vec<u64>, ApiError> {
    let start: Option<i64> = sqlx::query_scalar!(
        "select timestamp as \"timestamp!\" from ((select timestamp 
    from span
    where span.instance_id = $1
      and span.trace_id = $2 and span.timestamp >= $3)
      union all
    (select timestamp 
    from event
             where event.instance_id=$1
                 and event.trace_id=$2)
    order by timestamp limit 1);",
        trace_id.instance_id.instance_id,
        trace_id.trace_id,
        0
    )
    .fetch_optional(con)
    .map_err(|e| {
        SqlxError::from_sqlx_error(e, "getting trace timestamp start chunks for {trace_id:#?}")
    })
    .await?;
    let start = match start {
        None => {
            return Ok(vec![]);
        }
        Some(start) => start,
    };
    let end: i64 = sqlx::query_scalar!(
        "select timestamp as \"timestamp!\" from ((select coalesce(timestamp+span.duration, timestamp) as timestamp
    from span
    where span.instance_id = $1
      and span.trace_id = $2 and span.timestamp >= $3)
      union all
    (select timestamp 
    from event
             where event.instance_id=$1
                 and event.trace_id=$2)
    order by timestamp desc limit 1);",
        trace_id.instance_id.instance_id,
        trace_id.trace_id,
        0
    )
    .fetch_optional(con)
    .map_err(|e| {
        SqlxError::from_sqlx_error(e, "getting trace timestamp end chunks for {trace_id:#?}")
    })
    .await?
    .expect("end to exist");

    let mut timestamp_chunks: Vec<i64> = vec![start];
    loop {
        let last_timestamp = timestamp_chunks
            .last()
            .expect("to have at least one element, since we put one in");
        let timestamp: Option<i64> = sqlx::query_scalar!(
            "select timestamp as \"timestamp!\" from ((select timestamp 
    from span
    where span.instance_id = $1
      and span.trace_id = $2 and span.timestamp >= $3)
      union all
    (select timestamp 
    from event
             where event.instance_id=$1
                 and event.trace_id=$2 and event.timestamp >= $3)
    order by timestamp offset 300 limit 1);",
            trace_id.instance_id.instance_id,
            trace_id.trace_id,
            last_timestamp
        )
            .fetch_optional(con)
            .map_err(|e| SqlxError::from_sqlx_error(e, format!("getting trace timestamp middle chunks for {trace_id:#?} and timestamp: {last_timestamp}")))
            .await?;
        match timestamp {
            None => {
                if timestamp_chunks.last().expect("last to exist") != &end {
                    timestamp_chunks.push(end);
                }
                return Ok(timestamp_chunks
                    .into_iter()
                    .map(|e| handlers::db_i64_to_nanos(e).expect("timestamp chunks to fit i64"))
                    .collect());
            }
            Some(new_timestamp) => {
                timestamp_chunks.push(new_timestamp);
            }
        }
    }
}

#[instrument(level="error", skip_all, fields(trace_id=single_trace_query.trace_id))]
pub(crate) async fn ui_trace_chunk_list_get(
    Query(single_trace_query): Query<TraceId>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<u64>>, ApiError> {
    let con = app_state.con;
    let instance_id = single_trace_query.instance_id;
    let trace_id = single_trace_query.trace_id;
    let trace_ids = get_trace_timestamp_chunks(
        &con,
        TraceId {
            instance_id: instance_id,
            trace_id,
        },
    )
    .await?;
    Ok(Json(trace_ids))
}

#[instrument(level = "error", skip_all, err(Debug))]
pub(crate) async fn ui_trace_chunk_get(
    Query(single_trace_query): Query<SingleChunkTraceQuery>,
    State(app_state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let con = app_state.con;
    let instance_id = single_trace_query.trace_id.instance_id.instance_id;
    let trace_id = single_trace_query.trace_id.trace_id;
    let start_timestamp = u64_nanos_to_db_i64(single_trace_query.chunk_id.start_timestamp)?;
    let end_timestamp = u64_nanos_to_db_i64(single_trace_query.chunk_id.end_timestamp)?;
    info!("Getting single trace: {trace_id}");
    let raw_spans_from_db: Vec<RawDbSpan> = sqlx::query_as!(RawDbSpan,
        "select span.id,
                      span.timestamp,
                      span.parent_id,
                      span.duration,
                      span.name,
                      span.relocated,
                      COALESCE(span_key_value.key_values, '{}') as key_values
               from (select span.id,
                            span.timestamp,
                            span.parent_id,
                            span.duration,
                            span.name,
                            span.relocated
                     from span
                     where span.instance_id = $1
                       and span.trace_id = $2
                       and
                       -- (start inside window or end inside window)
                         ((
                                  (span.timestamp >= $3 and span.timestamp <= $4)
                                  or
                                  (span.duration is null or
                                   ((span.timestamp + span.duration) >= $3 and
                                    (span.timestamp + span.duration) <= $4))
                              )
                             -- or
                             -- start before window and end after window
                             or (span.timestamp <= $3 and
                                 (span.duration is null or (span.timestamp + span.duration) > $4)))) as span
                        left join (select span_id,
                                         json_object_agg(
                                                   span_key_value.key,
                                                   span_key_value.value
                                                   ) as key_values
                                   from span_key_value
                                   where span_key_value.instance_id = $1
                                     and span_key_value.trace_id = $2
                                   group by span_id) as span_key_value on span_key_value.span_id = span.id;",
        instance_id,
        trace_id,
        start_timestamp,
        end_timestamp
    )
        .fetch_all(&con)
        .map_err(|e| {
            SqlxError::from_sqlx_error(e, format!("getting single trace span data using {instance_id}, {trace_id}, {start_timestamp}, {end_timestamp}"))
        })
        .await?;

    let raw_events_from_db: Vec<RawDbEvent> = sqlx::query_as!(RawDbEvent,
        "select event.span_id, event.message, event.severity as \"severity: String\", event.relocated, event.timestamp, COALESCE(event_key_value.key_values, '{}') as key_values
from (select *
      from event
      where event.instance_id = $1
        and event.trace_id = $2
        and event.timestamp >= $3
        and event.timestamp <= $4) as event
         left join (select span_id,
                           event_id,
                           json_object_agg(
                                    event_key_value.key,
                                    event_key_value.value
                                    ) as key_values
                    from event_key_value
                    where event_key_value.instance_id = $1
                      and event_key_value.trace_id = $2
                    group by event_id, span_id) as event_key_value 
                   on event_key_value.span_id = event.span_id and event_key_value.event_id = event.id;",
        instance_id,
        trace_id,
        start_timestamp,
        end_timestamp
    )
        .fetch_all(&con)
        .map_err(|e| {
            SqlxError::from_sqlx_error(e, format!("getting single trace span data using {instance_id}, {trace_id}, {start_timestamp}, {end_timestamp}"))
        })
        .await?;

    let mut events_by_span_id: HashMap<i64, Vec<Event>> =
        raw_events_from_db
            .into_iter()
            .fold(HashMap::new(), |mut acc, e| {
                let entry = acc.entry(e.span_id).or_insert(Vec::new());
                entry.push(Event {
                    timestamp: e.timestamp as u64,
                    message: e.message,
                    severity: Severity::from_str(&e.severity).expect("severity to be valid"),
                    relocated: e.relocated,
                    key_values: serde_json::from_value(e.key_values)
                        .expect("event key value to be valid"),
                });
                acc
            });

    let spans: Vec<Span> = raw_spans_from_db
        .into_iter()
        .map(|s| Span {
            id: s.id,
            timestamp: s.timestamp as u64,
            parent_id: s.parent_id,
            duration: s.duration.map(|e| e as u64),
            name: s.name,
            relocated: s.relocated,
            events: events_by_span_id.remove(&s.id).unwrap_or_default(),
            key_values: serde_json::from_value(s.key_values).expect("span key value to be valid"),
        })
        .collect();

    info!("Got it, compressing");
    let lg_window_size = 21;
    let quality = 4;
    let json = serde_json::to_string(&spans).expect("to be able to serialize response");
    let mut input =
        brotli::CompressorReader::new(json.as_bytes(), 4096, quality as u32, lg_window_size as u32);
    let mut resp: Vec<u8> = Vec::with_capacity(10 * crate::BYTES_IN_1MB);
    input.read_to_end(&mut resp).unwrap();
    info!("Compressed, sending");
    Ok((
        StatusCode::OK,
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/json; charset=UTF-8",
            ),
            (axum::http::header::CONTENT_ENCODING, "br"),
        ],
        resp,
    ))
}
