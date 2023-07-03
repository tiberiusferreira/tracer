use crate::otel_trace_processing::{DbEvent, DbSpan};
use crate::{otel_trace_processing, BYTES_IN_1MB};
use api_structs::{ApiTraceGridRow, SearchFor, Span, Summary, SummaryRequest};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;
use sqlx::{Error, FromRow, PgPool};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::ops::Deref;
use tokio::task::JoinHandle;
use tracing::instrument::Instrumented;
use tracing::{error, info, info_span, instrument, Instrument};

#[derive(Debug, Clone, Serialize)]
struct RawDbSummary {
    service_name: String,
    top_level_span_name: String,
    total_traces: i64,
    total_traces_with_error: i64,
    longest_trace_id: i64,
    longest_trace_duration: i64,
    longest_trace_duration_service_name: String,
}

#[instrument(skip_all)]
async fn traces_summary(
    axum::extract::State(con): axum::extract::State<PgPool>,
    _summary_request: Json<SummaryRequest>,
) -> Result<Json<Vec<Summary>>, ApiError> {
    let summary_data = sqlx::query_as!(
        RawDbSummary,
        "with trace_services_summary as (select trace.service_name,
                                       trace.top_level_span_name,
                                       COUNT(trace.timestamp)        as total_traces,
                                       SUM((has_errors = true)::INT) as total_traces_with_error,
                                       MAX(duration)
                                                                     as longest_trace_duration
                                from trace
                                group by trace.service_name, trace.top_level_span_name)
select trace_services_summary.service_name,
       trace_services_summary.top_level_span_name,
       total_traces            as \"total_traces!\",
       total_traces_with_error as \"total_traces_with_error!\",
       trace.id                as \"longest_trace_id!\",
       trace.service_name      as \"longest_trace_duration_service_name!\",
       trace.duration          as \"longest_trace_duration!\"
from trace_services_summary
         join lateral (select id, trace.service_name, duration
                       from trace
                       where trace.service_name = trace_services_summary.service_name
                         and trace.top_level_span_name = trace_services_summary.top_level_span_name
                         and trace.duration = trace_services_summary.longest_trace_duration
                       limit 1) trace on true
order by service_name, total_traces_with_error desc, total_traces desc;"
    )
    .fetch_all(&con)
    .await?;
    let summary_data: Vec<Summary> = summary_data
        .into_iter()
        .map(|s| Summary {
            service_name: s.service_name,
            top_level_span_name: s.top_level_span_name,
            total_traces: s.total_traces,
            total_traces_with_error: s.total_traces_with_error,
            longest_trace_id: u64::try_from(s.longest_trace_id).expect("trace_id to fit u64"),
            longest_trace_duration: u64::try_from(s.longest_trace_duration)
                .expect("trace duration to fit u64"),
        })
        .collect();
    Ok(Json(summary_data))
}

#[instrument(skip_all)]
pub fn start(con: PgPool, api_port: u16) -> JoinHandle<()> {
    info!("Starting API");
    if std::fs::read("./tracer-ui/dist/index.html").is_err() {
        panic!("Failed to read ./tracer-ui/dist/index.html");
    }
    let serve_ui = tower_http::services::ServeDir::new("./tracer-ui/dist").fallback(
        tower_http::services::ServeFile::new("./tracer-ui/dist/index.html"),
    );

    let app = axum::Router::new()
        .route("/api/ready", axum::routing::get(ready))
        .route(
            "/api/traces-grid",
            axum::routing::post(traces_grid_with_search),
        )
        .route("/api/summary", axum::routing::post(traces_summary))
        .route("/api/trace", axum::routing::get(get_single_trace))
        .route("/collector/trace", axum::routing::post(post_single_trace))
        .route("/collector/status", axum::routing::post(post_status))
        .route(
            "/api/autocomplete-data",
            axum::routing::post(get_autocomplete_data),
        )
        .with_state(con)
        .fallback_service(serve_ui)
        .layer(tower_http::cors::CorsLayer::very_permissive());
    tokio::spawn(async move {
        axum::Server::bind(
            &format!("0.0.0.0:{}", api_port)
                .parse()
                .expect("should be able to api server desired address and port"),
        )
        .serve(app.into_make_service())
        .await
        .unwrap()
    })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RawGridErrorSample {
    span_name: String,
    span_attributes: HashMap<String, String>,
    event: String,
    event_attributes: HashMap<String, String>,
    event_timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GridErrorSample {
    span_name: String,
    span_attributes: HashMap<String, String>,
    event: String,
    event_attributes: HashMap<String, String>,
    event_timestamp_unix_ms: i64,
}

fn into_escaped_like_search(search_term: &str) -> String {
    let search_term = search_term.replace('%', "\\%");
    format!("%{}%", search_term)
}

const MAX_GRID_COL_LEN: usize = 30;

fn cut_matching_text_part(text: String, searched_term: String) -> String {
    let first_matching_bytes = text.find(&searched_term);
    match first_matching_bytes {
        None => text.chars().take(MAX_GRID_COL_LEN).collect(),
        Some(first_matching_bytes) => {
            let start = first_matching_bytes;
            let end = first_matching_bytes + searched_term.len();
            let slack = MAX_GRID_COL_LEN.saturating_sub(end - start);
            let mut new_start = start.saturating_sub(slack / 2);
            let mut new_end = end.saturating_add(slack / 2).min(text.len());
            while !text.is_char_boundary(new_start) {
                new_start -= new_start;
            }
            while !text.is_char_boundary(new_end) {
                new_end += new_end;
            }
            text[new_start..new_end].to_string()
        }
    }
}

#[cfg(test)]
#[test]
fn get_matching_text_part_works() {
    println!(
        "{}",
        cut_matching_text_part(
            "SQL query: SELECT * FROM (select disti".to_string(),
            "selec".to_string()
        )
    );
}

fn trim_and_highlight_search_term(
    specific_searched_term: &str,
    generic_search: &str,
    text: String,
) -> String {
    if !specific_searched_term.is_empty() {
        cut_matching_text_part(text, specific_searched_term.to_string())
    } else if !generic_search.is_empty() {
        cut_matching_text_part(text, generic_search.to_string())
    } else {
        text.chars().take(MAX_GRID_COL_LEN).collect()
    }
}

pub fn u64_to_naive_date_time(val: u64) -> Result<NaiveDateTime, ApiError> {
    let as_i64 = i64::try_from(val).map_err(|_| ApiError {
        code: StatusCode::BAD_REQUEST,
        message: "Invalid timestamp, doesnt fit into i64".to_string(),
    })?;
    let naive_date_time =
        NaiveDateTime::from_timestamp_opt(as_i64 / 1_000_000_000, (as_i64 % 1_000_000_000) as u32)
            .expect("Value to fit");
    Ok(naive_date_time)
}

#[derive(Debug, Clone)]
struct QueryReadyParameters {
    from: NaiveDateTime,
    to: NaiveDateTime,
    min_duration: i64,
    max_duration: Option<i64>,
    min_warn_count: Option<i64>,
    only_errors: Option<bool>,
    top_level_span: Option<String>,
    span_name: Option<String>,
    key: Option<String>,
    value: Option<String>,
    event_name: Option<String>,
    service_name: Option<String>,
}

impl QueryReadyParameters {
    pub fn from_search(search: SearchFor) -> Result<Self, ApiError> {
        let from = u64_to_naive_date_time(search.from_date_unix)?;
        let to = u64_to_naive_date_time(search.to_date_unix)?;
        let min_duration_micros = i64::try_from(search.min_duration).map_err(|_| ApiError {
            code: StatusCode::BAD_REQUEST,
            message: "Invalid trace min duration_micros".to_string(),
        })?;
        let max_duration_micros = search
            .max_duration
            .map(|max_duration_micros| {
                i64::try_from(max_duration_micros).map_err(|_| ApiError {
                    code: StatusCode::BAD_REQUEST,
                    message: "Invalid trace max duration_micros".to_string(),
                })
            })
            .transpose()?;
        let service_name = if search.service_name.is_empty() {
            None
        } else {
            Some(search.service_name)
        };
        let top_level_span = if search.top_level_span.is_empty() {
            None
        } else {
            Some(search.top_level_span)
        };
        let min_warns = if search.min_warns > 0 {
            Some(search.min_warns as i64)
        } else {
            None
        };
        let only_errors = if search.only_errors { Some(true) } else { None };
        let span_name = if search.span.is_empty() {
            None
        } else {
            Some(search.span)
        };
        let event_name = if search.event_name.is_empty() {
            None
        } else {
            Some(into_escaped_like_search(&search.event_name))
        };
        let key = if search.key.is_empty() {
            None
        } else {
            Some(search.key)
        };
        let value = if search.value.is_empty() {
            None
        } else {
            Some(into_escaped_like_search(&search.value))
        };
        Ok(QueryReadyParameters {
            key,
            value,
            top_level_span,
            span_name,
            event_name,
            from,
            to,
            min_duration: min_duration_micros,
            max_duration: max_duration_micros,
            min_warn_count: min_warns,
            service_name,
            only_errors,
        })
    }
}

#[derive(FromRow)]
pub struct RawDbTraceGrid {
    id: i64,
    timestamp: i64,
    duration: i64,
    service_name: String,
    has_errors: bool,
    warning_count: i64,
    top_level_span_name: String,
    key: Option<String>,
    value: Option<String>,
    span_name: Option<String>,
    event_name: Option<String>,
}

#[instrument(skip_all)]
pub async fn get_grid_data(
    con: &PgPool,
    search: SearchFor,
) -> Result<Vec<RawDbTraceGrid>, ApiError> {
    let query_params = QueryReadyParameters::from_search(search)?;
    info!("Query Parameters: {:#?}", query_params);
    let res = sqlx::query_as!(
        RawDbTraceGrid,
        "select distinct on (trace.timestamp, trace.id) trace.id,
                                                   trace.timestamp,
                                                   trace.duration,
                                                   trace.service_name,
                                                   trace.has_errors,
                                                   trace.warning_count,
                                                   trace.top_level_span_name,
                                                   COALESCE(event_key_value.key, span_key_value.key)   as \"key?\",
                                                   COALESCE(event_key_value.value, span_key_value.value)  as \"value?\",
                                                   span.name            as \"span_name?\",
                                                   event.name           as \"event_name?\"
    from trace
             left join span_key_value
                       on ($1::TEXT is not null and span_key_value.key = $1::TEXT)
                           and ($2::TEXT is null or span_key_value.value ilike $2::TEXT)
                           and span_key_value.trace_id = trace.id
             left join event_key_value
                       on ($1::TEXT is not null and event_key_value.key = $1::TEXT)
                           and ($2::TEXT is null or event_key_value.value ilike $2::TEXT)
                           and event_key_value.trace_id = trace.id
             left join span
                       on ($3::TEXT is not null and span.name = $3::TEXT)
                           and span.trace_id = trace.id
             left join event
                       on ($4::TEXT is not null and event.name ilike $4::TEXT)
                           and event.trace_id = trace.id
    where
      -- make sure if the user provided values, we treat is as an inner join
        ($1::TEXT is null or (span_key_value.key is not null or event_key_value.key is not null))
      and ($3::TEXT is null or span.id is not null)
      and ($4::TEXT is null or event.timestamp is not null)
      -- common filters
      and trace.timestamp >= $5::BIGINT
      and trace.timestamp <= $6::BIGINT
      and trace.duration >= $7::BIGINT
      and ($8::BIGINT is null or trace.duration <= $8::BIGINT)
      and ($9::TEXT is null or trace.service_name = $9::TEXT)
      and ($10::BOOL is null or trace.has_errors = $10::BOOL)
      and ($11::TEXT is null or trace.top_level_span_name = $11::TEXT)
      and ($12::BIGINT is null or trace.warning_count >= $12::BIGINT)
    order by trace.timestamp desc
    limit 100;",
        query_params.key,
        query_params.value,
        query_params.span_name,
        query_params.event_name,
        query_params.from.timestamp_nanos(),
        query_params.to.timestamp_nanos(),
        query_params.min_duration,
        query_params.max_duration,
        query_params.service_name,
        query_params.only_errors,
        query_params.top_level_span,
        query_params.min_warn_count,
    )
    .fetch_all(con)
    .await?;
    Ok(res)
}

#[instrument(skip_all)]
async fn get_service_names_autocomplete_data(
    con: &PgPool,
    query_params: &QueryReadyParameters,
) -> Result<Vec<String>, ApiError> {
    Ok(sqlx::query_scalar!(
        "select distinct trace.service_name from trace
            where
                 trace.timestamp >= $1::BIGINT
                 and trace.timestamp <= $2::BIGINT
                 and trace.duration  >= $3::BIGINT
                 and ($4::BIGINT is null or trace.duration <= $4::BIGINT)
                 and ($5::BIGINT is null or trace.warning_count >= $5::BIGINT)
                 and ($6::BOOLEAN is null or trace.has_errors = $6::BOOLEAN);",
        query_params.from.timestamp_nanos(),
        query_params.to.timestamp_nanos(),
        query_params.min_duration,
        query_params.max_duration,
        query_params.min_warn_count,
        query_params.only_errors,
    )
    .fetch_all(con)
    .await?)
}

#[instrument(skip_all)]
async fn get_top_level_span_autocomplete_data(
    con: &PgPool,
    query_params: &QueryReadyParameters,
) -> Result<Vec<String>, ApiError> {
    if let Some(service_name) = &query_params.service_name {
        let top_level_spans = sqlx::query_scalar!(
            "select distinct trace.top_level_span_name
                from trace
            where
                 trace.timestamp >= $1::BIGINT
                 and trace.timestamp <= $2::BIGINT
                 and trace.duration  >= $3::BIGINT
                 and ($4::BIGINT is null or trace.duration <= $4::BIGINT)
                 and ($5::BIGINT is null or trace.warning_count >= $5::BIGINT)
                 and ($6::BOOLEAN is null or trace.has_errors = $6::BOOLEAN)
                 and ($7::TEXT = trace.service_name);",
            query_params.from.timestamp_nanos(),
            query_params.to.timestamp_nanos(),
            query_params.min_duration,
            query_params.max_duration,
            query_params.min_warn_count,
            query_params.only_errors,
            service_name,
        )
        .fetch_all(con)
        .await?;
        Ok(top_level_spans)
    } else {
        Ok(vec![])
    }
}

struct SpanAndKeys {
    spans: Vec<String>,
    keys: Vec<String>,
}
#[instrument(skip_all)]
async fn get_span_and_keys_autocomplete_data(
    con: &PgPool,
    query_params: &QueryReadyParameters,
) -> Result<SpanAndKeys, ApiError> {
    if let (Some(service_name), Some(top_level_span_name)) =
        (&query_params.service_name, &query_params.top_level_span)
    {
        let spans = sqlx::query_scalar!(
            "select distinct span.name
                from trace
                inner join span on span.trace_id=trace.id
            where
                 trace.timestamp >= $1::BIGINT
                 and trace.timestamp <= $2::BIGINT
                 and trace.duration  >= $3::BIGINT
                 and ($4::BIGINT is null or trace.duration <= $4::BIGINT)
                 and ($5::BIGINT is null or trace.warning_count >= $5::BIGINT)
                 and ($6::BOOLEAN is null or trace.has_errors = $6::BOOLEAN)
                 and ($7::TEXT = trace.service_name)
                 and ($8::TEXT = trace.top_level_span_name);",
            query_params.from.timestamp_nanos(),
            query_params.to.timestamp_nanos(),
            query_params.min_duration,
            query_params.max_duration,
            query_params.min_warn_count,
            query_params.only_errors,
            service_name,
            top_level_span_name
        )
        .fetch_all(con)
        .instrument(info_span!("get_span_autocomplete"));
        let span_keys = sqlx::query_scalar!(
            "select distinct span_key_value.key
                    from trace
                    inner join span_key_value
                        on span_key_value.trace_id=trace.id
                where
                     trace.timestamp >= $1::BIGINT
                     and trace.timestamp <= $2::BIGINT
                     and trace.duration  >= $3::BIGINT
                     and ($4::BIGINT is null or trace.duration <= $4::BIGINT)
                     and ($5::BIGINT is null or trace.warning_count >= $5::BIGINT)
                     and ($6::BOOLEAN is null or trace.has_errors = $6::BOOLEAN)
                     and ($7::TEXT = trace.service_name)
                     and ($8::TEXT = trace.top_level_span_name)
                     and span_key_value.user_generated=true;",
            query_params.from.timestamp_nanos(),
            query_params.to.timestamp_nanos(),
            query_params.min_duration,
            query_params.max_duration,
            query_params.min_warn_count,
            query_params.only_errors,
            service_name,
            top_level_span_name
        )
        .fetch_all(con)
        .instrument(info_span!("get_span_key_autocomplete"));
        let event_keys = sqlx::query_scalar!(
            "select distinct event_key_value.key
                    from trace
                    inner join event_key_value
                        on event_key_value.trace_id=trace.id
                where
                     trace.timestamp >= $1::BIGINT
                     and trace.timestamp <= $2::BIGINT
                     and trace.duration  >= $3::BIGINT
                     and ($4::BIGINT is null or trace.duration <= $4::BIGINT)
                     and ($5::BIGINT is null or trace.warning_count >= $5::BIGINT)
                     and ($6::BOOLEAN is null or trace.has_errors = $6::BOOLEAN)
                     and ($7::TEXT = trace.service_name)
                     and ($8::TEXT = trace.top_level_span_name)
                     and event_key_value.user_generated=true;",
            query_params.from.timestamp_nanos(),
            query_params.to.timestamp_nanos(),
            query_params.min_duration,
            query_params.max_duration,
            query_params.min_warn_count,
            query_params.only_errors,
            service_name,
            top_level_span_name
        )
        .fetch_all(con)
        .instrument(info_span!("get_event_key_autocomplete"));
        let (spans, span_keys, event_keys) = tokio::try_join!(spans, span_keys, event_keys)?;
        let mut key_set: HashSet<String> = span_keys.into_iter().collect();
        key_set.extend(event_keys);
        Ok(SpanAndKeys {
            spans,
            keys: key_set.into_iter().collect(),
        })
    } else {
        Ok(SpanAndKeys {
            spans: vec![],
            keys: vec![],
        })
    }
}

#[instrument(skip_all)]
async fn get_autocomplete_data(
    axum::extract::State(con): axum::extract::State<PgPool>,
    search_for: Json<SearchFor>,
) -> Result<Json<api_structs::KeySpans>, ApiError> {
    let query_params = QueryReadyParameters::from_search(search_for.deref().clone())?;
    let closure_query_params = query_params.clone();
    let closure_con = con.clone();
    let service_names_fut: Instrumented<JoinHandle<Result<Vec<String>, ApiError>>> =
        tokio::spawn(async move {
            get_service_names_autocomplete_data(&closure_con, &closure_query_params).await
        })
        .in_current_span();
    let closure_query_params = query_params.clone();
    let closure_con = con.clone();
    let top_lvl_span_fut: Instrumented<JoinHandle<Result<Vec<String>, ApiError>>> =
        tokio::spawn(async move {
            get_top_level_span_autocomplete_data(&closure_con, &closure_query_params).await
        })
        .in_current_span();
    let closure_query_params = query_params.clone();
    let closure_con = con.clone();
    let span_and_keys_fut: Instrumented<JoinHandle<Result<SpanAndKeys, ApiError>>> =
        tokio::spawn(async move {
            get_span_and_keys_autocomplete_data(&closure_con, &closure_query_params).await
        })
        .in_current_span();

    let (service_names, top_level_spans, spans_and_keys) =
        tokio::try_join!(service_names_fut, top_lvl_span_fut, span_and_keys_fut).map_err(|e| {
            error!("{:?}", e);
            ApiError {
                code: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Internal error!".to_string(),
            }
        })?;
    let spans_and_keys = spans_and_keys?;
    Ok(Json(api_structs::KeySpans {
        service_names: service_names?,
        top_level_spans: top_level_spans?,
        spans: spans_and_keys.spans,
        keys: spans_and_keys.keys,
    }))
}
#[instrument(skip_all)]
async fn traces_grid_with_search(
    axum::extract::State(con): axum::extract::State<PgPool>,
    search_for: Json<SearchFor>,
) -> Result<Json<Vec<ApiTraceGridRow>>, ApiError> {
    let resp = get_grid_data(&con, search_for.0.clone()).await?;

    let resp: Vec<ApiTraceGridRow> = resp
        .into_iter()
        .map(|e| ApiTraceGridRow {
            id: u64::try_from(e.id).expect("trace_id to fit u64"),
            has_errors: e.has_errors,
            service_name: e.service_name,
            top_level_span_name: e.top_level_span_name,
            duration_ns: u64::try_from(e.duration).expect("duration to fit u64"),
            timestamp: u64::try_from(e.timestamp).expect("creation timestamp to fit u64"),
            key: e.key,
            value: e.value,
            span: e.span_name.map(|e| {
                trim_and_highlight_search_term(&search_for.span, &search_for.service_name, e)
            }),
            event: e.event_name,
            warning_count: u32::try_from(e.warning_count).expect("warning count to fit u32"),
        })
        .collect();
    Ok(Json(resp))
}

struct RawDbSpan {
    id: i64,
    timestamp: i64,
    name: String,
    duration: i64,
    parent_id: Option<i64>,
    span_key_values: JsonValue,
    events: JsonValue,
}

#[derive(Debug)]
pub struct ApiError {
    pub code: StatusCode,
    pub message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.code, self.message).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(value: Error) -> Self {
        error!("Error during api request: {:#?}", value);
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "DB error when handling the request".to_string(),
        }
    }
}

#[instrument(skip_all)]
async fn post_status(
    axum::extract::State(con): axum::extract::State<PgPool>,
    status: Json<api_structs::exporter::StatusData>,
) -> Result<(), ApiError> {
    println!("{:#?}", status.0);
    Ok(())
}

#[instrument(skip_all)]
async fn post_single_trace(
    axum::extract::State(con): axum::extract::State<PgPool>,
    trace: Json<api_structs::exporter::Trace>,
) -> Result<(), ApiError> {
    println!("New trace!");
    let trace = trace.0;
    let mut span_id_to_idx =
        trace
            .children
            .iter()
            .enumerate()
            .fold(HashMap::new(), |mut acc, (idx, curr)| {
                acc.insert(curr.id, idx + 2);
                acc
            });
    span_id_to_idx.insert(trace.id, 1);
    let mut spans = vec![];
    let mut root_events = vec![];
    for (idx, e) in trace.events.into_iter().enumerate() {
        root_events.push(DbEvent {
            id: i64::try_from(idx + 1).expect("idx to fit i64"),
            timestamp: i64::try_from(e.timestamp).expect("timestamp to fit i64"),
            name: e.name,
            key_values: vec![],
            severity: otel_trace_processing::Level::Info,
        });
    }
    spans.push(DbSpan {
        id: 1,
        timestamp: i64::try_from(trace.start).expect("timestamp to fit i64"),
        parent_id: None,
        name: trace.name.clone(),
        duration: i64::try_from(trace.duration).expect("timestamp to fit i64"),
        key_values: vec![],
        events: root_events,
    });
    for span in trace.children.into_iter() {
        let mut events = vec![];
        for (idx, e) in span.events.into_iter().enumerate() {
            events.push(DbEvent {
                id: i64::try_from(idx + 1).expect("idx to fit i64"),
                timestamp: i64::try_from(e.timestamp).expect("timestamp to fit i64"),
                name: e.name,
                key_values: vec![],
                severity: otel_trace_processing::Level::Info,
            });
        }
        spans.push(DbSpan {
            id: i64::try_from(*span_id_to_idx.get(&span.id).expect("span id to exist"))
                .expect("usize to fit i64"),
            timestamp: i64::try_from(span.start).expect("timestamp to fit i64"),
            parent_id: Some(
                i64::try_from(
                    *span_id_to_idx
                        .get(&span.parent_id)
                        .expect("parent id to exist"),
                )
                .expect("parent id to fit i64"),
            ),
            name: span.name,
            duration: i64::try_from(span.duration).expect("duration to fit i64"),
            key_values: vec![],
            events,
        });
    }
    let data = crate::otel_trace_processing::DbReadyTraceData {
        timestamp: i64::try_from(trace.start).expect("timestamp to fit i64"),
        service_name: trace.service_name,
        duration: i64::try_from(trace.duration).expect("duration to fit i64"),
        top_level_span_name: trace.name,
        has_errors: false,
        warning_count: 0,
        spans,
        span_plus_events_count: 0,
    };
    match crate::otel_trace_processing::store_trace(con, data).await {
        Ok(id) => {
            println!("Inserted: {}", id);
        }
        Err(e) => {
            println!("{:#?}", e);
        }
    }
    Ok(())
}
#[instrument(skip_all, fields(trace_id=trace_id.trace_id))]
async fn get_single_trace(
    axum::extract::Query(trace_id): axum::extract::Query<api_structs::TraceId>,
    axum::extract::State(con): axum::extract::State<PgPool>,
) -> Result<impl IntoResponse, ApiError> {
    let trace_id = trace_id.trace_id;
    info!("Getting single trace: {trace_id}");
    let trace_from_db = sqlx::query_as!(RawDbSpan, "with event_kv_by_span_event as (select event_key_value.span_id,
                                                      event_key_value.event_id,
                                                      json_agg(json_build_object('key',
                                                                                 event_key_value.key,
                                                                                 'user_generated',
                                                                                 event_key_value.user_generated,
                                                                                 'value',
                                                                                 event_key_value.value)) as key_vals
                                               from event_key_value
                                               where event_key_value.trace_id = $1
                                               group by event_key_value.span_id, event_key_value.event_id),
                    event_with_kv_by_span as (select event.span_id,
                                                     COALESCE(jsonb_agg(json_build_object('timestamp',
                                                                                          event.timestamp,
                                                                                          'name',
                                                                                          event.name,
                                                                                          'severity',
                                                                                          event.severity,
                                                                                          'key_values',
                                                                                          COALESCE(event_kv_by_span_event.key_vals, '[]'))),
                                                              '[]') as events
                                              from event
                                                       left join event_kv_by_span_event on
                                                          event.trace_id = $1 and
                                                          event.span_id = event_kv_by_span_event.span_id and
                                                          event.id = event_kv_by_span_event.event_id
                                              where event.trace_id = $1
                                              group by event.span_id),
                    span_kv_by_id as (select span_key_value.span_id,
                                             jsonb_agg(json_build_object('key',
                                                                        span_key_value.key,
                                                                        'user_generated',
                                                                        span_key_value.user_generated,
                                                                        'value',
                                                                        span_key_value.value)) as key_vals
                                      from span_key_value
                                      where span_key_value.trace_id = $1
                                      group by span_key_value.span_id),
                    span_with_events as (select span.id,
                                                span.timestamp,
                                                span.name,
                                                span.duration,
                                                span.parent_id,
                                                COALESCE(
                                                        span_kv_by_id.key_vals,
                                                        '[]') as span_key_values,
                                                COALESCE(event_with_kv_by_span.events, '[]') as events
                                         from span
                                                  left join event_with_kv_by_span on
                                                     span.trace_id = $1 and
                                                     span.id = event_with_kv_by_span.span_id
                                                  left join span_kv_by_id on span_kv_by_id.span_id = span.id
                                         where span.trace_id = $1
                                         group by span.id, span.timestamp, span.name, span.duration, span.parent_id,
                                                  event_with_kv_by_span.events, span_kv_by_id.key_vals)
               select span_with_events.id,
                      span_with_events.timestamp,
                      span_with_events.name,
                      span_with_events.duration,
                      span_with_events.parent_id,
                      span_with_events.span_key_values as \"span_key_values!\",
                      span_with_events.events          as \"events!\"
               from span_with_events;",
        trace_id,
    )
            .fetch_all(&con)
            .await?;
    let resp = trace_from_db
        .into_iter()
        .map(|span| Span {
            id: u64::try_from(span.id).expect("span.id to fit u64"),
            name: span.name,
            timestamp: u64::try_from(span.timestamp).expect("unix timestamp to fit u64"),
            duration: u64::try_from(span.duration).expect("span duration to fit u64"),
            parent_id: span
                .parent_id
                .map(|ts| u64::try_from(ts).expect("span parent_id to fit u64")),
            key_values: serde_json::from_value(span.span_key_values)
                .expect("db to generate valid json"),
            events: serde_json::from_value(span.events).expect("db to generate valid json"),
        })
        .collect::<Vec<Span>>();
    info!("Got it, compressing");
    let lg_window_size = 21;
    let quality = 4;
    let json = serde_json::to_string(&resp).expect("to be able to serialize response");
    let mut input =
        brotli::CompressorReader::new(json.as_bytes(), 4096, quality as u32, lg_window_size as u32);
    let mut resp: Vec<u8> = Vec::with_capacity(10 * BYTES_IN_1MB);
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

async fn ready() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=UTF-8",
        )],
        "ok".to_string(),
    )
}
