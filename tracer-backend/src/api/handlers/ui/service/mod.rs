use crate::api::state::AppState;
use crate::api::ApiError;
use api_structs::Endpoint;
use axum::extract::State;
use axum::Json;
use tracing::instrument;

#[instrument(level = "error", skip_all, err(Debug))]
pub(crate) async fn ui_service_filter_post(
    State(_app_state): State<AppState>,
    Json(_new_filter): Json<api_structs::ui::service::NewFiltersRequest>,
) -> Result<(), ApiError> {
    unimplemented!()
}

/// Used to get information of one of the current services we data for, current or past
#[instrument(skip_all)]
pub async fn get(
    State(_app_state): State<AppState>,
) -> Result<Json<<api_structs::ui::service::GetService as Endpoint>::ResponseBody>, ApiError> {
    unimplemented!()
}
