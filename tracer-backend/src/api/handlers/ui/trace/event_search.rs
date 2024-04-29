use crate::api::handlers::ui::trace::{RawDbEvent, RawDbSpan};
use crate::api::state::AppState;
use crate::api::{u64_nanos_to_db_i64, ApiError};
use api_structs::instance::update::Location;
use api_structs::time_conversion::{time_from_nanos, time_to_nanos_u64};
use api_structs::ui::trace::search::event::{Event, TraceEventSearch, TraceEventSearchUrlEncoded};
use api_structs::ui::trace::spans::TraceId;
use api_structs::Severity;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use backtraced_error::SqlxError;
use futures::TryFutureExt;
use sqlx::PgPool;
use std::collections::HashMap;
use std::io::Read;
use std::str::FromStr;
use tracing::{info, instrument};

#[instrument(skip_all)]
pub async fn trace_keys(
    State(app_state): State<AppState>,
    Json(trace_event_search): Json<TraceId>,
) -> Result<axum::Json<Vec<String>>, ApiError> {
    let res = sqlx::query_scalar!(
        "select distinct event_key_value.key
from event_key_value
where instance_id = $1
  and trace_id = $2;",
        trace_event_search.instance_id.instance_id,
        trace_event_search.trace_id as i32
    )
    .fetch_all(&app_state.con)
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, "trace_keys"))?;
    Ok(Json(res))
}

#[instrument(skip_all)]
pub async fn search(
    State(app_state): State<AppState>,
    Query(trace_event_search): Query<TraceEventSearchUrlEncoded>,
) -> Result<axum::Json<Vec<Event>>, ApiError> {
    let con = app_state.con;
    let trace_event_search: TraceEventSearch =
        serde_json::from_str(&trace_event_search.trace_event_search).map_err(|_| ApiError {
            code: StatusCode::BAD_REQUEST,
            message: "Invalid value".to_string(),
        })?;
    let trace_chunk = trace_event_search.chunk;
    let instance_id = trace_chunk.trace_id.instance_id.instance_id;
    let trace_id = trace_chunk.trace_id.trace_id;
    let start_timestamp = time_from_nanos(trace_chunk.chunk_id.start_timestamp);
    let end_timestamp = time_from_nanos(trace_chunk.chunk_id.end_timestamp);
    let severity: Vec<crate::api::handlers::Severity> = trace_event_search
        .severity
        .iter()
        .map(|e| crate::api::handlers::Severity::from(e.clone()))
        .collect();
    let severity = if severity.is_empty() {
        None
    } else {
        Some(severity)
    };
    let substring = trace_event_search.substring.map(|e| format!("%{}%", e));
    let key_0 = trace_event_search.key_0.map(|e| format!("{}", e));
    let value_0 = trace_event_search.value_0.map(|e| format!("%{}%", e));
    let key_1 = trace_event_search.key_1.map(|e| format!("{}", e));
    let value_1 = trace_event_search.value_1.map(|e| format!("%{}%", e));

    let raw_events_from_db: Vec<RawDbEvent> = sqlx::query_as!(RawDbEvent,
            "select event.span_id,
           event.message,
           event.severity                             as \"severity: String\",
           event.timestamp,
           COALESCE(event_key_value.key_values, '{}') as key_values,
           event.module,
           event.filename,
           event.line
    from (select *
          from event
          where event.instance_id = $1
            and event.trace_id = $2
            and event.timestamp >= $3
            and event.timestamp <= $4
            and ($5::severity_level[] is null or severity = ANY($5::severity_level[]))
            and ($6::TEXT is null or message ilike $6::TEXT)) as event
             left join (select span_id,
                               event_id,
                               json_object_agg(
                                       event_key_value.key,
                                       event_key_value.value
                               ) as key_values
                        from event_key_value
                        where event_key_value.instance_id = $1
                          and event_key_value.trace_id = $2
                          and ($7::text is null or (event_key_value.key = $7 and ($8::text is null or event_key_value.value ilike $8)))
                          and ($9::text is null or (event_key_value.key = $9 and ($10::text is null or event_key_value.value ilike $10)))
                        group by event_id, span_id) as event_key_value
                       on event_key_value.span_id = event.span_id and event_key_value.event_id = event.id
                    where (($7 is null and $9 is null) or event_key_value.key_values is not null) order by event.timestamp limit 250;",
            instance_id,
            trace_id as i32,
            start_timestamp,
            end_timestamp,
            &severity as &Option<Vec<crate::api::handlers::Severity>>,
            substring,
            key_0,
            value_0,
            key_1,
            value_1
        )
            .fetch_all(&con)
            .map_err(|e| {
                SqlxError::from_sqlx_error(e, format!("getting searched event using {instance_id}, {trace_id}, {start_timestamp}, {end_timestamp}"))
            })
            .await?;

    let events: Vec<Event> = raw_events_from_db
        .into_iter()
        .map(|e| Event {
            timestamp: time_to_nanos_u64(e.timestamp),
            message: e.message,
            severity: Severity::from_str(&e.severity).expect("severity to be valid"),
            key_values: serde_json::from_value(e.key_values).expect("event key value to be valid"),
            location: Location {
                module: e.module,
                filename: e.filename,
                line: e.line.map(|e| e as u32),
            },
        })
        .collect();

    Ok(axum::Json(events))
}
