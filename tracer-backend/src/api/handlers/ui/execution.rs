use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::execution::{Attribute, Execution};
use api_structs::instance::update::ReplayData;
use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use gel_io_recorder::Parameter;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetExecutionQueryParameter {
    pub id: Uuid,
}

pub(crate) async fn get_single_execution(
    State(app_state): State<AppState>,
    query: Query<GetExecutionQueryParameter>,
) -> Result<Json<Execution>, ApiError> {
    let id = query.id;
    let db = app_state.execution_io_provider.database();
    let mut tx = db.transaction_start().await;
    let execution: Option<Execution> = tx
        .query_optional(
            "select Execution{
  id,
  size_bytes,
  service_instance_id := .service_instance.id,
  service_env := .service_instance.service.env,
  service_name := .service_instance.service.name,
  started_at,
  last_seen_at,
  ended,
  replay_data,
  attributes := .<execution[is ExecutionAttribute]{
    name,
    value := ._value
  }
} filter .id=<uuid>$id",
            HashMap::from([(
                "id".to_string(),
                Parameter::Uuid {
                    val: id,
                    cast_to_table: None,
                },
            )]),
        )
        .await?;
    let execution = execution.ok_or_else(|| ApiError {
        code: StatusCode::NOT_FOUND,
        message: "invalid execution id".to_string(),
    })?;
    Ok(Json(execution))
}
