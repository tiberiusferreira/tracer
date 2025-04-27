use crate::api::ApiError;
use crate::api::state::AppState;
use axum::Json;
use axum::extract::State;
use tracing::instrument;

#[instrument(level = "error", skip_all, err(Debug))]
pub(crate) async fn a(
    State(_app_state): State<AppState>,
    Json(_new_filter): Json<api_structs::ui::service::NewFiltersRequest>,
) -> Result<(), ApiError> {
    unimplemented!()
}
