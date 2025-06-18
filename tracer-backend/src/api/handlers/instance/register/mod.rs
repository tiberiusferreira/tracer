use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::ServiceId;
use api_structs::instance::registration::RegistrationResponse;
use axum::Json;
use axum::extract::State;
use std::collections::HashMap;

use gel_io_recorder::Parameter;
use gel_tokio::Queryable;
use serde::{Deserialize, Serialize};

#[derive(Queryable)]
struct InstanceInsertionData {
    service_instance_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Id {
    pub id: uuid::Uuid,
}
async fn register_instance(
    tx: &mut gel_io_recorder::Transaction,
    env: &str,
    service: &str,
) -> Result<InstanceInsertionData, gel_io_recorder::Error> {
    let params = HashMap::from([
        ("env".to_string(), Parameter::from(env)),
        ("service".to_string(), Parameter::from(service)),
    ]);

    let service_id: Option<Id> = tx
        .query_optional(
            "with
    env := <str>$env,
    name := <str>$service,
select Service{
  id
} filter .env = env and .name = name;",
            params,
        )
        .await?;
    let service_id = match service_id {
        None => {
            let map = HashMap::from([
                ("env", Parameter::from(env)),
                ("name", Parameter::from(service)),
            ]);
            tx.insert("Service", map).await?
        }
        Some(id) => id.id,
    };
    let map = HashMap::from([("service", Parameter::from((service_id, "Service")))]);
    let service_instance_id = tx.insert("ServiceInstance", map).await?;
    Ok(InstanceInsertionData {
        service_instance_id,
    })
}

pub async fn handler(
    app_state: State<AppState>,
    service_id: Json<ServiceId>,
) -> Result<Json<RegistrationResponse>, ApiError> {
    let service_id = service_id.0;
    let db = app_state.execution_io_provider.database.clone();
    let mut tx = db.transaction_start().await?;
    let instance_insertion_data =
        register_instance(&mut tx, &service_id.env, &service_id.name).await?;
    tx.commit().await?;
    Ok(Json(RegistrationResponse {
        instance_id: instance_insertion_data.service_instance_id,
    }))
}
