use crate::api::handlers::instance::connect::ChangeFilterInternalRequest;
use crate::api::state::AppState;
use crate::api::{ApiError, AppStateError, ServiceInAppStateButNotDBError};
use api_structs::ui::service::Instance;
use api_structs::ServiceId;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use backtraced_error::SqlxError;
use tracing::instrument;

#[instrument(level = "error", skip_all, err(Debug))]
pub(crate) async fn ui_service_filter_post(
    State(app_state): State<AppState>,
    Json(new_filter): Json<api_structs::ui::service::NewFiltersRequest>,
) -> Result<(), ApiError> {
    let handle = {
        match app_state
            .services_runtime_stats
            .write()
            .get(&new_filter.service_id)
        {
            None => {
                return Err(ApiError {
                    code: StatusCode::NOT_FOUND,
                    message: format!("Service doesn't exist: {:#?}", new_filter.service_id),
                });
            }
            Some(handle) => match handle.instances.get(&new_filter.instance_id) {
                None => {
                    return Err(ApiError {
                        code: StatusCode::NOT_FOUND,
                        message: format!(
                            "Service exists, but no instance given id {} running",
                            new_filter.instance_id
                        ),
                    });
                }
                Some(instance_state) => instance_state.see_handle.clone(),
            },
        }
    };
    return match handle
        .send(ChangeFilterInternalRequest {
            filters: new_filter.filters,
        })
        .await
    {
        Ok(_sent) => Ok(()),
        Err(_e) => Err(ApiError {
            code: StatusCode::BAD_REQUEST,
            message: format!(
                "Instance with id {} is no longer connected",
                new_filter.instance_id
            ),
        }),
    };
}

/// Used to list the current services we have data for, current or past
#[instrument(level = "error", skip_all)]
pub async fn ui_service_list_get(
    State(app_state): State<AppState>,
) -> Result<Json<Vec<ServiceId>>, ApiError> {
    let con = &app_state.con;
    let services = sqlx::query_as!(ServiceId, "select env, name from service;")
        .fetch_all(con)
        .await
        .map_err(|e| SqlxError::from_sqlx_error(e, "getting service from DB"))?;
    Ok(Json(services))
}

/// Used to get information of one of the current services we data for, current or past
#[instrument(level = "error", skip_all)]
pub(crate) async fn ui_service_overview_get(
    service_id: Query<ServiceId>,
    State(app_state): State<AppState>,
) -> Result<Json<api_structs::ui::service::ServiceOverview>, ApiError> {
    let service_id = service_id.0;
    let service_data = app_state
        .services_runtime_stats
        .read()
        .clone()
        .get(&service_id)
        .cloned();
    let service_data = match service_data {
        None => {
            return Err(ApiError {
                code: StatusCode::NOT_FOUND,
                message: "Service not found".to_string(),
            });
        }
        Some(service_data) => service_data,
    };
    let config =
        crate::database::service_initialization::get_service_config(&app_state.con, &service_id)
            .await?
            .ok_or_else(|| {
                AppStateError::ServiceInAppStateButNotDB(ServiceInAppStateButNotDBError::new(
                    &service_id,
                ))
            })?;
    let api_service_data = api_structs::ui::service::ServiceOverview {
        service_id,
        alert_config: config.alert_config,
        instances: service_data
            .instances
            .iter()
            .map(|(id, data)| Instance {
                id: *id,
                rust_log: data.rust_log.clone(),
                last_seen_secs_ago: data.last_seen.elapsed().as_secs(),
                profile_data: data.profile_data.clone(),
            })
            .collect(),
        service_data_over_time: service_data
            .service_data_points
            .iter()
            .map(|d| api_structs::ui::service::ServiceDataOverTime {
                timestamp: d.timestamp,
                instance_id: d.instance_id,
                export_buffer_stats: d.export_buffer_stats.clone(),
                active_traces: d.active_traces.clone(),
                finished_traces: d.finished_traces.clone(),
                orphan_events: d.orphan_events.clone(),
                traces_budget_usage: d.budget_usage.traces_usage.clone(),
                orphan_events_budget_usage: d.budget_usage.orphan_events_usage,
            })
            .collect(),
    };
    Ok(Json(api_service_data))
}
