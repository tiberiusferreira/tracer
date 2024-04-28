use std::collections::HashMap;
use std::io::Read;
use std::str::FromStr;

use api_structs::instance::update::Location;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use futures::TryFutureExt;
use sqlx::PgPool;
use tracing::{info, instrument};

use api_structs::ui::trace::spans::{SingleChunkTraceQuery, Span, TraceId};
use api_structs::ui::trace::TraceHeaderAndSpans;
use api_structs::Severity;
use backtraced_error::SqlxError;

use crate::api::handlers::ui::trace::{RawDbEvent, RawDbSpan};
use crate::api::state::AppState;
use crate::api::{handlers, u64_nanos_to_db_i64, ApiError};

#[instrument(skip_all, fields(trace_id=trace_id.trace_id))]
pub async fn get_trace_timestamp_chunks(
    con: &PgPool,
    trace_id: TraceId,
) -> Result<Vec<u64>, ApiError> {
    // let start: Option<i64> = sqlx::query_scalar!(
    //     "select timestamp as \"timestamp!\" from ((select timestamp
    // from span
    // where span.instance_id = $1
    //   and span.trace_id = $2 and span.timestamp >= $3)
    //   union all
    // (select timestamp
    // from event
    //          where event.instance_id=$1
    //              and event.trace_id=$2)
    // order by timestamp limit 1);",
    //     trace_id.instance_id.instance_id,
    //     trace_id.trace_id,
    //     0
    // )
    // .fetch_optional(con)
    // .map_err(|e| {
    //     SqlxError::from_sqlx_error(e, "getting trace timestamp start chunks for {trace_id:#?}")
    // })
    // .await?;
    // let start = match start {
    //     None => {
    //         return Ok(vec![]);
    //     }
    //     Some(start) => start,
    // };
    // let end: i64 = sqlx::query_scalar!(
    //     "select timestamp as \"timestamp!\" from ((select coalesce(timestamp+span.duration, timestamp) as timestamp
    // from span
    // where span.instance_id = $1
    //   and span.trace_id = $2 and span.timestamp >= $3)
    //   union all
    // (select timestamp
    // from event
    //          where event.instance_id=$1
    //              and event.trace_id=$2)
    // order by timestamp desc limit 1);",
    //     trace_id.instance_id.instance_id,
    //     trace_id.trace_id,
    //     0
    // )
    // .fetch_optional(con)
    // .map_err(|e| {
    //     SqlxError::from_sqlx_error(e, "getting trace timestamp end chunks for {trace_id:#?}")
    // })
    // .await?
    // .expect("end to exist");
    //
    // let mut timestamp_chunks: Vec<i64> = vec![start];
    // loop {
    //     let last_timestamp = timestamp_chunks
    //         .last()
    //         .expect("to have at least one element, since we put one in");
    //     let timestamp: Option<i64> = sqlx::query_scalar!(
    //         "select timestamp as \"timestamp!\" from ((select timestamp
    // from span
    // where span.instance_id = $1
    //   and span.trace_id = $2 and span.timestamp >= $3)
    //   union all
    // (select timestamp
    // from event
    //          where event.instance_id=$1
    //              and event.trace_id=$2 and event.timestamp >= $3)
    // order by timestamp offset 20000 limit 1);",
    //         trace_id.instance_id.instance_id,
    //         trace_id.trace_id,
    //         last_timestamp
    //     )
    //         .fetch_optional(con)
    //         .map_err(|e| SqlxError::from_sqlx_error(e, format!("getting trace timestamp middle chunks for {trace_id:#?} and timestamp: {last_timestamp}")))
    //         .await?;
    //     match timestamp {
    //         None => {
    //             if timestamp_chunks.last().expect("last to exist") != &end {
    //                 timestamp_chunks.push(end);
    //             }
    //             return Ok(timestamp_chunks
    //                 .into_iter()
    //                 .map(|e| handlers::db_i64_to_nanos(e).expect("timestamp chunks to fit i64"))
    //                 .collect());
    //         }
    //         Some(new_timestamp) => {
    //             timestamp_chunks.push(new_timestamp);
    //         }
    //     }
    // }
    unimplemented!()
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
pub(crate) async fn get_header_and_spans(
    Query(single_trace_query): Query<TraceId>,
    State(app_state): State<AppState>,
) -> Result<Json<TraceHeaderAndSpans>, ApiError> {
    //     let con = app_state.con;
    //     let instance_id = single_trace_query.instance_id.instance_id;
    //     let trace_id = single_trace_query.trace_id;
    //     // let start_timestamp = u64_nanos_to_db_i64(single_trace_query.chunk_id.start_timestamp)?;
    //     // let end_timestamp = u64_nanos_to_db_i64(single_trace_query.chunk_id.end_timestamp)?;
    //     struct RawHeader {
    //         pub top_level_span_name: String,
    //         pub start: i64,
    //         pub duration: Option<i64>,
    //     }
    //
    //     let header = sqlx::query_as!(
    //         RawHeader,
    //         "select top_level_span_name, timestamp as start, duration
    // from trace
    // where instance_id = $1
    //   and id = $2;",
    //         instance_id,
    //         trace_id
    //     )
    //     .fetch_one(&con)
    //     .await
    //     .map_err(|e| SqlxError::from_sqlx_error(e, "getting raw header"))?;
    //
    //     let max_span: i64 = sqlx::query_scalar!(
    //         "select max(span.timestamp + coalesce(span.duration, 0)) as \"max_span!\"
    //                     from span
    //                     where instance_id = $1
    //                       and trace_id = $2
    //                     group by instance_id, trace_id;",
    //         instance_id,
    //         trace_id
    //     )
    //     .fetch_one(&con)
    //     .await
    //     .map_err(|e| SqlxError::from_sqlx_error(e, "getting max span"))?;
    //     let max_event: i64 = sqlx::query_scalar!(
    //         "select coalesce(max(event.timestamp), 0) as \"max_event!\"
    //                     from event
    //                     where instance_id = $1
    //                       and trace_id = $2
    //                     group by instance_id, trace_id;",
    //         instance_id,
    //         trace_id
    //     )
    //     .fetch_optional(&con)
    //     .await
    //     .map_err(|e| SqlxError::from_sqlx_error(e, "getting max event"))?
    //     .unwrap_or(0);
    //
    //     info!("Getting single trace: {trace_id}");
    //     let raw_spans_from_db: Vec<RawDbSpan> = sqlx::query_as!(RawDbSpan,
    //         "select span.id,
    //                       span.timestamp,
    //                       span.parent_id,
    //                       span.duration,
    //                       span.name,
    //                       COALESCE(span_key_value.key_values, '{}') as key_values,
    //                       span.module,
    //                       span.filename,
    //                       span.line
    //                from (select span.id,
    //                             span.timestamp,
    //                             span.parent_id,
    //                             span.duration,
    //                             span.name,
    //                             span.module,
    //                             span.filename,
    //                             span.line
    //                      from span
    //                      where span.instance_id = $1
    //                        and span.trace_id = $2
    //                        )
    //                         as span
    //                         left join (select span_id,
    //                                          json_object_agg(
    //                                                    span_key_value.key,
    //                                                    span_key_value.value
    //                                                    ) as key_values
    //                                    from span_key_value
    //                                    where span_key_value.instance_id = $1
    //                                      and span_key_value.trace_id = $2
    //                                    group by span_id) as span_key_value on span_key_value.span_id = span.id;",
    //         instance_id,
    //         trace_id,
    //         // start_timestamp,
    //         // end_timestamp
    //     )
    //         .fetch_all(&con)
    //         .map_err(|e| {
    //             SqlxError::from_sqlx_error(e, format!("getting single trace span data using {instance_id}, {trace_id}"))
    //         })
    //         .await?;
    //
    //     let spans: Vec<Span> = raw_spans_from_db
    //         .into_iter()
    //         .map(|s| Span {
    //             id: s.id,
    //             timestamp: s.timestamp as u64,
    //             parent_id: s.parent_id,
    //             duration: s.duration.map(|e| e as u64),
    //             name: s.name,
    //             key_values: serde_json::from_value(s.key_values).expect("span key value to be valid"),
    //             location: Location {
    //                 module: s.module,
    //                 filename: s.filename,
    //                 line: s.line.map(|e| e as u32),
    //             },
    //         })
    //         .collect();
    //     let duration = header.duration.unwrap_or_else(|| max_span.max(max_event)) as u64;
    //     Ok(Json(TraceHeaderAndSpans {
    //         top_level_span_name: header.top_level_span_name,
    //         start: header.start as u64,
    //         duration,
    //         spans,
    //     }))
    unimplemented!()
}
