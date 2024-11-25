use crate::api::handlers::instance::register::service_initialization::{Error, ServiceDbId};
use crate::api::state::AppState;
use crate::api::{state, ApiError, LiveServiceInstance};
use api_structs::instance::registration::RegistrationResponse;
use api_structs::instance::update::ConfigChange;
use api_structs::{InstanceGlobalId, ServiceId};
use axum::extract::State;
use axum::Json;
use futures::StreamExt;
use sqlx::{Postgres, Transaction};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{info, instrument, trace, warn};
use tracked_error::{error_chain_to_pretty_formatted, SqlxError};

pub mod service_initialization;

#[instrument(skip_all)]
pub async fn handler(
    app_state: State<AppState>,
    service_id: Json<ServiceId>,
) -> Result<Json<RegistrationResponse>, ApiError> {
    let service_id = service_id.0;
    info!(service_name=service_id.name, service_env=?service_id.env,  "registration request for service");
    let mut transaction = app_state.con.begin().await.map_err(SqlxError::from)?;
    let instance_id =
        match service_initialization::get_service_db_id(&mut transaction, &service_id).await? {
            None => {
                let service_db_id = service_initialization::insert_service(
                    &mut transaction,
                    &service_id,
                    "info".to_string(),
                )
                .await?;
                insert_new_instance(&mut transaction, service_db_id).await?
            }
            Some(service_db_id) => insert_new_instance(&mut transaction, service_db_id).await?,
        };

    let log_filter = crate::api::handlers::instance::database::get_instance_service_log_filter(
        &mut transaction,
        instance_id,
    )
    .await?
    .expect("log filter to exist, we just created the instance");
    transaction.commit().await.map_err(SqlxError::from)?;
    Ok(Json(RegistrationResponse {
        instance_id,
        log_filter,
    }))
}

#[instrument(skip_all)]
pub async fn insert_new_instance(
    con: &mut Transaction<'static, Postgres>,
    service_db_id: ServiceDbId,
) -> Result<InstanceGlobalId, SqlxError> {
    let global_id: uuid::Uuid = sqlx::query_scalar!(
        "insert into instance (service_id, global_id) values ($1, $2) returning global_id;",
        service_db_id,
        uuid::Uuid::new_v4()
    )
    .fetch_one(&mut **con)
    .await?;
    Ok(global_id)
}
