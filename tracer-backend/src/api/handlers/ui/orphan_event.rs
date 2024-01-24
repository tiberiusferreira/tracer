use crate::api::handlers::{db_i64_to_nanos, nanos_to_db_i64, Severity};
use crate::api::state::AppState;
use crate::api::ApiError;
use api_structs::ui::orphan_events::{OrphanEvent, ServiceOrphanEventsRequest};
use axum::extract::{Query, State};
use axum::Json;
use backtraced_error::SqlxError;
use tracing::instrument;

#[instrument(level = "error", skip_all)]
pub(crate) async fn ui_orphan_events_get(
    service_log_request: Query<ServiceOrphanEventsRequest>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<OrphanEvent>>, ApiError> {
    let from_timestamp = nanos_to_db_i64(service_log_request.from_date_unix)?;
    let to_timestamp = nanos_to_db_i64(service_log_request.to_date_unix)?;
    let service_name = &service_log_request.service_id.name;
    let env = &service_log_request.service_id.env;
    pub struct DbLog {
        pub timestamp: i64,
        pub severity: Severity,
        pub message: Option<String>,
        pub key_vals: sqlx::types::JsonValue,
    }
    let service_list: Vec<DbLog> = sqlx::query_as!(
        DbLog,
        r#"select orphan_event.timestamp,
       severity       as "severity: Severity",
       orphan_event.message,
       COALESCE(json_object_agg(
                orphan_event_key_value.key,
                orphan_event_key_value.value
                               )
                filter ( where orphan_event_key_value.key is not null),
                '{}') as key_vals
from orphan_event
         left join orphan_event_key_value
             on orphan_event_key_value.orphan_event_id = orphan_event.id
where orphan_event.env = $1
  and orphan_event.service_name = $2
  and orphan_event.timestamp >= $3
  and orphan_event.timestamp <= $4
group by orphan_event.timestamp, orphan_event.severity, orphan_event.message
order by timestamp desc
limit 100000"#,
        env.to_string(),
        service_name,
        from_timestamp,
        to_timestamp,
    )
    .fetch_all(&app_state.con)
    .await
    .map_err(|e| {
        SqlxError::from_sqlx_error(
            e,
            format!("getting logs using  {env}, {service_name}, {from_timestamp}, {to_timestamp}"),
        )
    })?;

    Ok(Json(
        service_list
            .into_iter()
            .map(|e| OrphanEvent {
                timestamp: db_i64_to_nanos(e.timestamp).expect("db timestamp to fit u64"),
                severity: e.severity.to_api(),
                message: e.message,
                key_vals: serde_json::from_value(e.key_vals)
                    .expect("to be able to deserialize event kv from DB"),
            })
            .collect(),
    ))
}
