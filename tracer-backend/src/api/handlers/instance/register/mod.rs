use crate::api::state::AppState;
use crate::api::ApiError;
use api_structs::instance::registration::RegistrationResponse;
use api_structs::{InstanceGlobalId, ServiceId};
use axum::extract::State;
use axum::Json;
use edgedb_codegen::edgedb_query;
use sqlx::{Postgres, Transaction};
use tracing::{info, instrument};
use tracked_error::{EdgeDBError, SqlxError};

edgedb_query!(
    insert_service,
    "
with
  env := <str>$env,
  name := <str>$name,
  service := (
      insert Service{
              env := env,
              name := name,
              log_filter := (
                insert LogFilter {
                  log_filter := 'info'
                }
              )
            }
      unless conflict on (.env, .name)
      else
        (select Service)
  ),
  service_instance := (
    insert ServiceInstance{
      service := service,
      latest_log_filter := (
        insert LogFilter {
                  log_filter := 'info'
            }
      )
    }
  )
select {
  service := service{log_filter: {log_filter}},
  service_existed := (service not in Service),
  service_instance := service_instance
};
"
);

#[instrument(skip_all)]
pub async fn handler(
    app_state: State<AppState>,
    service_id: Json<ServiceId>,
) -> Result<Json<RegistrationResponse>, ApiError> {
    let service_id = service_id.0;
    info!(service.name=service_id.name, service.env=?service_id.env,  "registration request for service");
    let mut tx = app_state
        .edgedb_client
        .transaction()
        .await
        .map_err(|e| EdgeDBError::from(e))?;

    let service = insert_service::transaction(
        &mut tx,
        &insert_service::Input {
            env: service_id.env.to_string(),
            name: service_id.name.clone(),
        },
    )
    .await
    .map_err(|e| EdgeDBError::from(e))?;
    tx.commit().await.map_err(|e| EdgeDBError::from(e))?;
    let service_instance_id = service.service_instance.id;
    let log_filter = service.service.log_filter.log_filter;
    info!(
        service.already_existed = service.service_existed,
        instance.id = service_instance_id.to_string(),
        log_filter = log_filter,
        "instance registered"
    );
    Ok(Json(RegistrationResponse {
        instance_id: service_instance_id,
        log_filter,
    }))
}
