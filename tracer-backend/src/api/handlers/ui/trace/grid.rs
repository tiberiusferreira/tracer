use crate::api::state::AppState;
use crate::api::{handlers, u64_nanos_to_db_i64, ApiError};
use api_structs::time_conversion::{time_from_nanos, time_to_nanos_u64};
use api_structs::ui::trace::grid::{Autocomplete, SearchFor, TraceGridResponse, TraceGridRow};
use api_structs::ui::trace::spans::TraceId;
use api_structs::{Env, InstanceGlobalId, ServiceId};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::NaiveDateTime;
use futures::TryFutureExt;
use sqlx::{FromRow, PgPool};
use tokio::task::JoinHandle;
use tracing::instrument::Instrumented;
use tracing::{error, info, info_span, instrument, Instrument};
use tracked_error::SqlxError;
use uuid::Uuid;

#[instrument(level = "error", skip_all)]
pub async fn ui_trace_grid_get(
    State(app_state): State<AppState>,
    search_for: Query<SearchFor>,
) -> Result<Json<TraceGridResponse>, ApiError> {
    let con = app_state.con;
    let resp = get_grid_data(&con, search_for.0.clone()).await?;
    Ok(Json(resp))
}

#[instrument(skip_all)]
pub async fn get_grid_data(con: &PgPool, search: SearchFor) -> Result<TraceGridResponse, ApiError> {
    // let query_params = QueryReadyParameters::from_search(search)?;
    // info!("Query Parameters: {:#?}", query_params);
    // let count: i64 = sqlx::query_scalar!(
    //         "select COUNT(*) as \"count!\"
    // from trace_cache
    //     inner join trace on trace.instance_id=trace_cache.instance_id and trace.id=trace_cache.trace_id
    // where trace.updated_at >= $1::timestamp
    //   and trace.updated_at <= $2::timestamp
    //   and ($3::TEXT is null or trace_cache.service_name = $3::TEXT)
    //   and ($4::TEXT is null or trace_cache.top_level_span_name = $4::TEXT)
    //   and (trace_cache.duration_nanos >= $5::BIGINT or trace_cache.duration_nanos is null)
    //   and ($6::BIGINT is null or trace_cache.duration_nanos is null or trace_cache.duration_nanos <= $6::BIGINT)
    //   and ($7::BOOL is null or trace_cache.has_errors = $7::BOOL)
    //   and ($8::BIGINT is null or trace_cache.warnings >= $8::BIGINT);",
    //         query_params.from,
    //         query_params.to,
    //         query_params.service_name,
    //         query_params.top_level_span,
    //         query_params.min_duration,
    //         query_params.max_duration,
    //         query_params.only_errors,
    //         query_params.min_warn_count,
    //     )
    //     .fetch_one(con)
    //     .instrument(info_span!("get_row_count"))
    //     .map_err(|e| SqlxError::from_sqlx_error(e, "getting grid count"))
    //     .await?;
    // let res: Vec<RawDbTraceGrid> = sqlx::query_as!(
    //         RawDbTraceGrid,
    //         "select trace_cache.env,
    //        trace_cache.service_name,
    //        trace_cache.instance_id,
    //        trace.id,
    //        trace_cache.timestamp,
    //        trace_cache.top_level_span_name,
    //        trace_cache.duration_nanos,
    //        trace.spans_produced,
    //        trace_cache.spans_stored,
    //        trace.events_produced,
    //        trace.events_dropped_by_sampling,
    //        trace_cache.events_stored,
    //        trace_cache.size_bytes,
    //        trace_cache.warnings,
    //        trace_cache.has_errors,
    //        trace.updated_at
    // from trace_cache
    //     inner join trace on trace.instance_id=trace_cache.instance_id and trace.id=trace_cache.trace_id
    // where trace.updated_at >= $1::timestamp
    //   and trace.updated_at <= $2::timestamp
    //   and ($3::TEXT is null or trace_cache.service_name = $3::TEXT)
    //   and ($4::TEXT is null or trace_cache.top_level_span_name = $4::TEXT)
    //   and (trace_cache.duration_nanos >= $5::BIGINT or trace_cache.duration_nanos is null)
    //   and ($6::BIGINT is null or trace_cache.duration_nanos is null or trace_cache.duration_nanos <= $6::BIGINT)
    //   and ($7::BOOL is null or trace_cache.has_errors = $7::BOOL)
    //   and ($8::BIGINT is null or trace_cache.warnings >= $8::BIGINT)
    // order by trace.updated_at desc
    // limit 100;",
    //         query_params.from,
    //         query_params.to,
    //         query_params.service_name,
    //         query_params.top_level_span,
    //         query_params.min_duration,
    //         query_params.max_duration,
    //         query_params.only_errors,
    //         query_params.min_warn_count,
    //     )
    //     .fetch_all(con)
    //     .map_err(|e| SqlxError::from_sqlx_error(e, "getting grid data"))
    //     .await?;
    // let rows = res
    //     .into_iter()
    //     .map(|e| TraceGridRow {
    //         trace_id: TraceId {
    //             instance_id: InstanceId {
    //                 service_id: ServiceId {
    //                     name: e.service_name,
    //                     env: Env::from(e.env),
    //                 },
    //                 instance_id: e.instance_id,
    //             },
    //             trace_id: e.id as u32,
    //         },
    //         started_at: time_to_nanos_u64(e.timestamp),
    //         top_level_span_name: e.top_level_span_name,
    //         duration_ns: e
    //             .duration_nanos
    //             .map(|dur| handlers::db_i64_to_nanos(dur).expect("db duration to fit i64")),
    //         spans_produced: e.spans_produced as u64,
    //         events_produced: e.events_produced as u64,
    //         spans_stored: e.spans_stored as u64,
    //         events_dropped_by_sampling: e.events_dropped_by_sampling as u64,
    //         events_stored: e.events_stored as u64,
    //         size_bytes: e.size_bytes as u64,
    //         warnings: u32::try_from(e.warnings).expect("warning count to fit u32"),
    //         has_errors: e.has_errors,
    //         updated_at: time_to_nanos_u64(e.updated_at),
    //     })
    //     .collect();
    // let res = TraceGridResponse {
    //     rows,
    //     count: count as u32,
    // };
    // Ok(res)
    unimplemented!()
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
    service_name: Option<String>,
}

impl QueryReadyParameters {
    pub fn from_search(search: SearchFor) -> Result<Self, ApiError> {
        let from = time_from_nanos(search.from_date_unix);
        let to = time_from_nanos(search.to_date_unix);
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
    env: String,
    service_name: String,
    instance_id: Uuid,
    id: i32,
    timestamp: NaiveDateTime,
    top_level_span_name: String,
    duration_nanos: Option<i64>,
    spans_produced: i32,
    spans_stored: i32,
    events_produced: i32,
    events_dropped_by_sampling: i32,
    events_stored: i32,
    size_bytes: i32,
    warnings: i32,
    has_errors: bool,
    updated_at: NaiveDateTime,
}

#[instrument(skip_all)]
async fn get_top_level_span_autocomplete_data(
    con: &PgPool,
    query_params: &QueryReadyParameters,
) -> Result<Vec<String>, ApiError> {
    // if let Some(service_name) = &query_params.service_name {
    //     let top_level_spans = sqlx::query_scalar!(
    //         "select distinct trace_cache.top_level_span_name
    //               from trace_cache
    //            where trace_cache.timestamp >= $1::timestamp
    // and trace_cache.timestamp <= $2::timestamp
    // and ($3::TEXT is null or trace_cache.service_name = $3::TEXT)
    // and ($4::TEXT is null or trace_cache.top_level_span_name = $4::TEXT)
    // and trace_cache.duration_nanos >= $5::BIGINT
    // and ($6::BIGINT is null or trace_cache.duration_nanos <= $6::BIGINT)
    // and ($7::BOOL is null or trace_cache.has_errors = $7::BOOL)
    // and ($8::BIGINT is null or trace_cache.warnings >= $8::BIGINT);",
    //         query_params.from,
    //         query_params.to,
    //         service_name,
    //         query_params.top_level_span,
    //         query_params.min_duration,
    //         query_params.max_duration,
    //         query_params.only_errors,
    //         query_params.min_warn_count,
    //     )
    //     .fetch_all(con)
    //     .map_err(|e| {
    //         SqlxError::from_sqlx_error(
    //             e,
    //             format!("Getting top level span autocomplete data using: {query_params:#?}",),
    //         )
    //     })
    //     .await?;
    //     Ok(top_level_spans)
    // } else {
    //     Ok(vec![])
    // }
    unimplemented!()
}

#[instrument(level = "error", skip_all)]
pub(crate) async fn ui_trace_autocomplete_get(
    State(app_state): State<AppState>,
    search_for: Query<SearchFor>,
) -> Result<Json<Autocomplete>, ApiError> {
    let search_for = search_for.0;
    info!(?search_for);
    let con = app_state.con;
    let query_params = QueryReadyParameters::from_search(search_for)?;
    info!(?query_params);
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
    Ok(Json(Autocomplete {
        service_names: service_names?,
        top_level_spans: top_level_spans?,
    }))
}

#[instrument(skip_all)]
async fn get_service_names_autocomplete_data(
    con: &PgPool,
    query_params: &QueryReadyParameters,
) -> Result<Vec<String>, ApiError> {
    // Ok(sqlx::query_scalar!(
    //     "select distinct trace_cache.service_name from trace_cache
    //           where trace_cache.timestamp >= $1::timestamp
    // and trace_cache.timestamp <= $2::timestamp
    // and ($3::TEXT is null or trace_cache.service_name = $3::TEXT)
    // and ($4::TEXT is null or trace_cache.top_level_span_name = $4::TEXT)
    // and (trace_cache.duration_nanos is null or trace_cache.duration_nanos >= $5::BIGINT)
    // and ($6::BIGINT is null or trace_cache.duration_nanos <= $6::BIGINT)
    // and ($7::BOOL is null or trace_cache.has_errors = $7::BOOL)
    // and ($8::BIGINT is null or trace_cache.warnings >= $8::BIGINT);",
    //     query_params.from,
    //     query_params.to,
    //     query_params.service_name,
    //     query_params.top_level_span,
    //     query_params.min_duration,
    //     query_params.max_duration,
    //     query_params.only_errors,
    //     query_params.min_warn_count,
    // )
    // .fetch_all(con)
    // .map_err(|e| {
    //     SqlxError::from_sqlx_error(
    //         e,
    //         format!("getting service names autocomplete data using: {query_params:?}"),
    //     )
    // })
    // .await?)
    unimplemented!()
}
