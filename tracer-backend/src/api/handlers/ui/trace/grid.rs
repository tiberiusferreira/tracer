use crate::api::state::AppState;
use crate::api::ApiError;
use api_structs::time_conversion::time_from_nanos;
use api_structs::ui::trace::grid::{Autocomplete, SearchFor, TraceGridResponse};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::NaiveDateTime;
use sqlx::PgPool;
use tracing::instrument;

#[instrument(level = "error", skip_all)]
pub async fn ui_trace_grid_get(
    State(_app_state): State<AppState>,
    _search_for: Query<SearchFor>,
) -> Result<Json<TraceGridResponse>, ApiError> {
    // let con = app_state.con;
    // let resp = get_grid_data(&con, search_for.0.clone()).await?;
    // Ok(Json(resp))
    unimplemented!()
}

#[instrument(skip_all)]
pub async fn get_grid_data(
    _con: &PgPool,
    _search: SearchFor,
) -> Result<TraceGridResponse, ApiError> {
    unimplemented!()
}

#[derive(Debug, Clone)]
#[allow(unused)]
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
    #[allow(unused)]
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

#[instrument(skip_all)]
async fn get_top_level_span_autocomplete_data(
    _con: &PgPool,
    _query_params: &QueryReadyParameters,
) -> Result<Vec<String>, ApiError> {
    unimplemented!()
}

#[instrument(level = "error", skip_all)]
pub(crate) async fn ui_trace_autocomplete_get(
    State(_app_state): State<AppState>,
    _search_for: Query<SearchFor>,
) -> Result<Json<Autocomplete>, ApiError> {
    // let search_for = search_for.0;
    // info!(?search_for);
    // let con = app_state.con;
    // let query_params = QueryReadyParameters::from_search(search_for)?;
    // info!(?query_params);
    // let closure_query_params = query_params.clone();
    // let closure_con = con.clone();
    // let service_names_fut: Instrumented<JoinHandle<Result<Vec<String>, ApiError>>> =
    //     tokio::spawn(async move {
    //         get_service_names_autocomplete_data(&closure_con, &closure_query_params).await
    //     })
    //     .in_current_span();
    // let closure_query_params = query_params.clone();
    // let closure_con = con.clone();
    // let top_lvl_span_fut: Instrumented<JoinHandle<Result<Vec<String>, ApiError>>> =
    //     tokio::spawn(async move {
    //         get_top_level_span_autocomplete_data(&closure_con, &closure_query_params).await
    //     })
    //     .in_current_span();
    // let (service_names, top_level_spans) = tokio::try_join!(service_names_fut, top_lvl_span_fut)
    //     .map_err(|e| {
    //         error!("{:?}", e);
    //         ApiError {
    //             code: StatusCode::INTERNAL_SERVER_ERROR,
    //             message: "Internal error!".to_string(),
    //         }
    //     })?;
    // Ok(Json(Autocomplete {
    //     service_names: service_names?,
    //     top_level_spans: top_level_spans?,
    // }))
    unimplemented!()
}

#[instrument(skip_all)]
async fn get_service_names_autocomplete_data(
    _con: &PgPool,
    _query_params: &QueryReadyParameters,
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
