use crate::api::handlers::{
    instances_filter_post, instances_get, logs_get, logs_service_names_get,
};
use api_structs::exporter::LiveServiceInstance;
use api_structs::{SearchFor, Span};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::NaiveDateTime;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;
use sqlx::{Error, FromRow, PgPool};
use std::collections::HashMap;
use std::convert::Infallible;
use std::io::Read;
use std::ops::Deref;
use std::time::Duration;
use tokio::task::{spawn_local, JoinHandle};
use tracing::instrument::Instrumented;
use tracing::{debug, error, info, instrument, trace, Instrument};

pub mod handlers;

#[derive(Clone)]
pub struct AppState {
    // that holds some api specific state
    con: PgPool,
    live_instances: LiveInstances,
}

pub type ServiceName = String;
pub type InstanceId = i64;

pub struct ChangeFilterInternalRequest {
    filters: String,
}
#[derive(Clone)]
pub struct LiveInstances {
    pub trace_data:
        std::sync::Arc<parking_lot::RwLock<HashMap<ServiceName, Vec<LiveServiceInstance>>>>,
    pub see_handle: std::sync::Arc<
        parking_lot::RwLock<
            HashMap<InstanceId, tokio::sync::mpsc::Sender<ChangeFilterInternalRequest>>,
        >,
    >,
}

async fn sse_handler(
    State(app_state): State<AppState>,
    Path(instance_id): Path<i64>,
) -> axum::response::Sse<
    impl futures::stream::Stream<Item = Result<axum::response::sse::Event, Infallible>>,
> {
    trace!("New SSE connection request for {}", instance_id);
    let mut w_lock = app_state.live_instances.see_handle.write();
    let (s, r) = tokio::sync::mpsc::channel(1);
    if let Some(_existing) = w_lock.insert(instance_id, s) {
        error!("overwrote existing sse channel for {}", instance_id);
    }
    let stream = futures::stream::unfold(r, |mut r| async {
        let request = r.recv().await?;
        let data = api_structs::sse::SseRequest::NewFilter {
            filter: request.filters,
        };
        let see = axum::response::sse::Event::default().data(serde_json::to_string(&data).unwrap());
        Some((see, r))
    })
    .map(Ok);
    axum::response::sse::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

fn clean_up_service_instances_task(live_instances: LiveInstances) -> JoinHandle<()> {
    trace!("Starting clean_up_services_task");
    spawn_local(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        clean_up_service_instances(&live_instances);
    })
}

#[instrument(skip_all)]
fn clean_up_service_instances(live_instances: &LiveInstances) {
    trace!("cleaning up service");
    live_instances.see_handle.write().retain(|id, val| {
        if val.is_closed() {
            debug!("dropping sse_handle for instance with id: {id}");
            false
        } else {
            true
        }
    });
    live_instances
        .trace_data
        .write()
        .retain(|service_name, instance_list| {
            instance_list.retain(|single_instance| {
                let date = api_structs::time_conversion::time_from_nanos(
                    single_instance.last_seen_timestamp,
                );
                let last_seen_as_secs = chrono::Utc::now()
                    .naive_utc()
                    .signed_duration_since(date)
                    .num_seconds();
                if last_seen_as_secs > 5 * 60 {
                    debug!(
                        "dropping instance {} - id: {} - last seen {}s ago",
                        single_instance.service_name, single_instance.service_id, last_seen_as_secs
                    );
                    false
                } else {
                    true
                }
            });
            if instance_list.is_empty(){
                debug!(
                        "Last instance of {service_name} was dropped, removing it from list of services",
                    );
                false
            }else{
                true
            }

        });
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

    let app_state = AppState {
        con,
        live_instances: LiveInstances {
            trace_data: std::sync::Arc::new(parking_lot::RwLock::new(HashMap::new())),
            see_handle: std::sync::Arc::new(parking_lot::RwLock::new(HashMap::new())),
        },
    };
    let _clean_up_service_instances_task =
        clean_up_service_instances_task(app_state.live_instances.clone());

    let app = axum::Router::new()
        .route("/api/ready", axum::routing::get(ready))
        .route("/api/instances", axum::routing::get(instances_get))
        .route(
            "/api/logs/service_names",
            axum::routing::get(logs_service_names_get),
        )
        .route("/api/logs", axum::routing::get(logs_get))
        .route(
            "/api/instances/filter",
            axum::routing::post(instances_filter_post),
        )
        .route("/api/traces-grid", axum::routing::post(traces_grid_post))
        // .route("/api/summary", axum::routing::post(traces_summary))
        .route(
            "/api/trace_chunk_list",
            axum::routing::get(get_single_trace_chunk_list),
        )
        .route("/api/trace", axum::routing::get(get_single_trace))
        .route(
            "/collector/sse/:instance_id",
            axum::routing::get(sse_handler),
        )
        .route(
            "/collector/trace_data",
            axum::routing::post(handlers::collector_trace_data_post),
        )
        .route(
            "/api/autocomplete-data",
            axum::routing::post(autocomplete_data_post),
        )
        .with_state(app_state)
        .fallback_service(serve_ui)
        // 10 MB
        .layer(axum::extract::DefaultBodyLimit::max(1048576))
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

pub fn u64_nanos_to_db_i64(val: u64) -> Result<i64, ApiError> {
    let as_i64 = i64::try_from(val).map_err(|_| ApiError {
        code: StatusCode::BAD_REQUEST,
        message: "Invalid timestamp, doesnt fit into i64".to_string(),
    })?;
    Ok(as_i64)
}

#[derive(Debug, Clone)]
struct QueryReadyParameters {
    from: i64,
    to: i64,
    min_duration: i64,
    max_duration: Option<i64>,
    min_warn_count: Option<i64>,
    only_errors: Option<bool>,
    top_level_span: Option<String>,
    service_name: Option<String>,
}

impl QueryReadyParameters {
    pub fn from_search(search: SearchFor) -> Result<Self, ApiError> {
        let from = u64_nanos_to_db_i64(search.from_date_unix)?;
        let to = u64_nanos_to_db_i64(search.to_date_unix)?;
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
        Ok(QueryReadyParameters {
            top_level_span,
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
    service_id: i64,
    id: i64,
    service_name: String,
    timestamp: i64,
    top_level_span_name: String,
    duration: Option<i64>,
    warning_count: i64,
    has_errors: bool,
}

#[instrument(skip_all)]
pub async fn get_grid_data(
    con: &PgPool,
    search: SearchFor,
) -> Result<Vec<api_structs::ApiTraceGridRow>, ApiError> {
    let query_params = QueryReadyParameters::from_search(search)?;
    info!("Query Parameters: {:#?}", query_params);
    let res: Vec<RawDbTraceGrid> = sqlx::query_as!(
        RawDbTraceGrid,
        "select trace.service_id,
       trace.id,
       trace.service_name,
       trace.timestamp,
       trace.top_level_span_name,
       trace.duration,
       trace.warning_count,
       trace.has_errors
from trace
where trace.timestamp >= $1::BIGINT
  and trace.timestamp <= $2::BIGINT
  and ($3::TEXT is null or trace.service_name = $3::TEXT)
  and ($4::TEXT is null or trace.top_level_span_name = $4::TEXT)
  and (trace.duration >= $5::BIGINT or trace.duration is null)
  and ($6::BIGINT is null or trace.duration is null or trace.duration <= $6::BIGINT)
  and ($7::BOOL is null or trace.has_errors = $7::BOOL)
  and ($8::BIGINT is null or trace.warning_count >= $8::BIGINT)
order by trace.timestamp desc
limit 100;",
        query_params.from,
        query_params.to,
        query_params.service_name,
        query_params.top_level_span,
        query_params.min_duration,
        query_params.max_duration,
        query_params.only_errors,
        query_params.min_warn_count,
    )
    .fetch_all(con)
    .await?;
    let res = res
        .into_iter()
        .map(|e| api_structs::ApiTraceGridRow {
            service_id: e.service_id,
            id: e.id,
            service_name: e.service_name,
            timestamp: handlers::db_i64_to_nanos(e.timestamp).expect("db timestamp to fit i64"),
            top_level_span_name: e.top_level_span_name,
            duration_ns: e
                .duration
                .map(|dur| handlers::db_i64_to_nanos(dur).expect("db duration to fit i64")),
            warning_count: u32::try_from(e.warning_count).expect("warning count to fit u32"),
            has_errors: e.has_errors,
        })
        .collect();
    Ok(res)
}

#[instrument(skip_all)]
async fn get_service_names_autocomplete_data(
    con: &PgPool,
    query_params: &QueryReadyParameters,
) -> Result<Vec<String>, ApiError> {
    Ok(sqlx::query_scalar!(
        "select distinct trace.service_name from trace
            where trace.timestamp >= $1::BIGINT
  and trace.timestamp <= $2::BIGINT
  and ($3::TEXT is null or trace.service_name = $3::TEXT)
  and ($4::TEXT is null or trace.top_level_span_name = $4::TEXT)
  and trace.duration >= $5::BIGINT
  and ($6::BIGINT is null or trace.duration <= $6::BIGINT)
  and ($7::BOOL is null or trace.has_errors = $7::BOOL)
  and ($8::BIGINT is null or trace.warning_count >= $8::BIGINT);",
        query_params.from,
        query_params.to,
        query_params.service_name,
        query_params.top_level_span,
        query_params.min_duration,
        query_params.max_duration,
        query_params.only_errors,
        query_params.min_warn_count,
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
             where trace.timestamp >= $1::BIGINT
  and trace.timestamp <= $2::BIGINT
  and ($3::TEXT is null or trace.service_name = $3::TEXT)
  and ($4::TEXT is null or trace.top_level_span_name = $4::TEXT)
  and trace.duration >= $5::BIGINT
  and ($6::BIGINT is null or trace.duration <= $6::BIGINT)
  and ($7::BOOL is null or trace.has_errors = $7::BOOL)
  and ($8::BIGINT is null or trace.warning_count >= $8::BIGINT);",
            query_params.from,
            query_params.to,
            service_name,
            query_params.top_level_span,
            query_params.min_duration,
            query_params.max_duration,
            query_params.only_errors,
            query_params.min_warn_count,
        )
        .fetch_all(con)
        .await?;
        Ok(top_level_spans)
    } else {
        Ok(vec![])
    }
}

#[instrument(skip_all)]
async fn autocomplete_data_post(
    axum::extract::State(app_state): axum::extract::State<AppState>,
    search_for: Json<SearchFor>,
) -> Result<Json<api_structs::Autocomplete>, ApiError> {
    let con = app_state.con;
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
    let (service_names, top_level_spans) = tokio::try_join!(service_names_fut, top_lvl_span_fut)
        .map_err(|e| {
            error!("{:?}", e);
            ApiError {
                code: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Internal error!".to_string(),
            }
        })?;
    Ok(Json(api_structs::Autocomplete {
        service_names: service_names?,
        top_level_spans: top_level_spans?,
    }))
}
#[instrument(skip_all)]
async fn traces_grid_post(
    axum::extract::State(app_state): axum::extract::State<AppState>,
    search_for: Json<SearchFor>,
) -> Result<Json<Vec<api_structs::ApiTraceGridRow>>, ApiError> {
    let con = app_state.con;
    let resp = get_grid_data(&con, search_for.0.clone()).await?;
    Ok(Json(resp))
}

struct RawDbSpan {
    id: i64,
    timestamp: i64,
    parent_id: Option<i64>,
    duration: Option<i64>,
    name: String,
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

pub struct TraceId {
    pub service_id: i64,
    pub trace_id: i64,
}

#[instrument(skip_all, fields(trace_id=trace_id.trace_id))]
pub async fn get_trace_timestamp_chunks(
    con: &PgPool,
    trace_id: TraceId,
) -> Result<Vec<u64>, ApiError> {
    let start: Option<i64> = sqlx::query_scalar!(
        "select timestamp as \"timestamp!\" from ((select timestamp 
    from span
    where span.service_id = $1
      and span.trace_id = $2 and span.timestamp >= $3)
      union all
    (select timestamp 
    from event
             where event.service_id=$1
                 and event.trace_id=$2)
    order by timestamp limit 1);",
        trace_id.service_id,
        trace_id.trace_id,
        0
    )
    .fetch_optional(con)
    .await?;
    let end: i64 = sqlx::query_scalar!(
        "select timestamp as \"timestamp!\" from ((select timestamp 
    from span
    where span.service_id = $1
      and span.trace_id = $2 and span.timestamp >= $3)
      union all
    (select timestamp 
    from event
             where event.service_id=$1
                 and event.trace_id=$2)
    order by timestamp desc limit 1);",
        trace_id.service_id,
        trace_id.trace_id,
        0
    )
    .fetch_optional(con)
    .await?
    .expect("end to exist");
    let start = match start {
        None => {
            return Ok(vec![]);
        }
        Some(start) => start,
    };
    let mut timestamp_chunks: Vec<i64> = vec![start];
    loop {
        let last_timestamp = timestamp_chunks
            .last()
            .expect("to have at least one element, since we put one in");
        let timestamp: Option<i64> = sqlx::query_scalar!(
            "select timestamp as \"timestamp!\" from ((select timestamp 
    from span
    where span.service_id = $1
      and span.trace_id = $2 and span.timestamp >= $3)
      union all
    (select timestamp 
    from event
             where event.service_id=$1
                 and event.trace_id=$2 and event.timestamp >= $3)
    order by timestamp offset 10 limit 1);",
            trace_id.service_id,
            trace_id.trace_id,
            last_timestamp
        )
        .fetch_optional(con)
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

#[instrument(skip_all, fields(trace_id=single_trace_query.trace_id))]
async fn get_single_trace_chunk_list(
    axum::extract::Query(single_trace_query): axum::extract::Query<api_structs::TraceId>,
    axum::extract::State(app_state): axum::extract::State<AppState>,
) -> Result<Json<Vec<u64>>, ApiError> {
    let con = app_state.con;
    let service_id = single_trace_query.service_id;
    let trace_id = single_trace_query.trace_id;
    let trace_ids = get_trace_timestamp_chunks(
        &con,
        TraceId {
            service_id,
            trace_id,
        },
    )
    .await?;
    Ok(Json(trace_ids))
}

#[instrument(skip_all)]
async fn get_single_trace(
    axum::extract::Query(single_trace_query): axum::extract::Query<
        api_structs::SingleChunkTraceQuery,
    >,
    axum::extract::State(app_state): axum::extract::State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let con = app_state.con;
    let service_id = single_trace_query.trace_id.service_id;
    let trace_id = single_trace_query.trace_id.trace_id;
    let start_timestamp = u64_nanos_to_db_i64(single_trace_query.chunk_id.start_timestamp)?;
    let end_timestamp = u64_nanos_to_db_i64(single_trace_query.chunk_id.end_timestamp)?;
    info!("Getting single trace: {trace_id}");
    let trace_from_db: Vec<RawDbSpan> = sqlx::query_as!(RawDbSpan, "select span_events.id,
                                                      span_events.timestamp,
                                                      span_events.parent_id,
                                                      span_events.duration,
                                                      span_events.name,
                                                      COALESCE(jsonb_agg(
                                                               json_build_object('timestamp',
                                                                                 span_events.event_timestamp,
                                                                                 'name',
                                                                                 span_events.event_name,
                                                                                 'severity',
                                                                                 span_events.severity
                                                               )
                                                                        )
                                                               filter ( where span_events.event_timestamp is not null),
                                                               '[]') as events
                                               from (select span.id,
                                                            span.timestamp,
                                                            span.parent_id,
                                                            span.duration,
                                                            span.name,
                                                            event.timestamp as event_timestamp,
                                                            event.name      as event_name,
                                                            event.severity
                                                     from span
                                                              left join event
                                                                        on event.service_id = span.service_id and
                                                                           event.trace_id = span.trace_id
                                                                            and event.span_id = span.id
                                                     where span.service_id = $1
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
                                                        or (span.timestamp <= $3 and (span.duration is null or (span.timestamp + span.duration) > $4)))
                                                         and (event.timestamp is null or (event.timestamp >= $3 and event.timestamp <= $4))
                                                     order by event.timestamp) span_events

                                               group by span_events.id, span_events.timestamp, span_events.parent_id,
                                                        span_events.duration, span_events.name
                                                order by span_events.timestamp;",
        service_id,
        trace_id,
        start_timestamp,
        end_timestamp
    )
            .fetch_all(&con)
            .await?;
    let resp = trace_from_db
        .into_iter()
        .map(|span| Span {
            id: span.id,
            name: span.name,
            timestamp: handlers::db_i64_to_nanos(span.timestamp).expect("db timestamp to fit u64"),
            duration: span
                .duration
                .map(|d| handlers::db_i64_to_nanos(d).expect("db durations to fit u64")),
            parent_id: span.parent_id,
            events: serde_json::from_value(span.events).expect("db to generate valid json"),
        })
        .collect::<Vec<Span>>();
    info!("Got it, compressing");
    let lg_window_size = 21;
    let quality = 4;
    let json = serde_json::to_string(&resp).expect("to be able to serialize response");
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
