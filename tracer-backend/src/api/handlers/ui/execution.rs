use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::execution::{Attribute, Execution};
use api_structs::instance::update::ReplayDataFragment;
use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use gel_io_recorder::Parameter;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetExecutionQueryParameter {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbExecution {
    pub external_id: Uuid,
    pub size_bytes: u64,
    pub service_instance_id: Uuid,
    pub service_env: String,
    pub service_name: String,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ended: bool,
    pub replay_data: Vec<ReplayDataFragment>,
    pub attributes: Vec<Attribute>,
}
pub(crate) async fn get_single_execution(
    State(app_state): State<AppState>,
    query: Query<GetExecutionQueryParameter>,
) -> Result<Json<Execution>, ApiError> {
    let execution_external_id = query.id;
    let db = app_state.execution_io_provider.database();
    let mut tx = db.transaction_start().await?;
    let execution: Option<DbExecution> = tx
        .query_optional(
            "select Execution{
  external_id,
  size_bytes,
  service_instance_id := .service_instance.id,
  service_env := .service_instance.service.env,
  service_name := .service_instance.service.name,
  started_at,
  last_seen_at,
  ended,
  replay_data := .<execution[is ReplayFragment].replay_data,
  attributes := .<execution[is ExecutionAttribute]{
    name,
    value := ._value
  }
} filter .external_id=<uuid>$external_id",
            HashMap::from([(
                "external_id".to_string(),
                Parameter::from(execution_external_id),
            )]),
        )
        .await?;
    let execution = execution.ok_or_else(|| ApiError {
        code: StatusCode::NOT_FOUND,
        message: "invalid execution id".to_string(),
    })?;
    let mut replay_data = ReplayDataFragment {
        input: None,
        io_providers_events: Default::default(),
    };
    for fragment in execution.replay_data {
        if let Some(input) = fragment.input {
            assert!(replay_data.input.is_none());
            replay_data.input = Some(input);
        }
        for (key, value) in fragment.io_providers_events {
            let entry = replay_data.io_providers_events.entry(key).or_default();
            entry.extend(value);
        }
    }
    Ok(Json(Execution {
        external_id: execution.external_id,
        size_bytes: execution.size_bytes,
        service_instance_id: execution.service_instance_id,
        service_env: execution.service_env,
        service_name: execution.service_name,
        started_at: execution.started_at,
        last_seen_at: execution.last_seen_at,
        ended: execution.ended,
        replay_data,
        attributes: execution.attributes,
    }))
}
