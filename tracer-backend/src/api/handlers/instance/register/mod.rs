use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::ServiceId;
use api_structs::instance::registration::RegistrationResponse;
use axum::Json;
use axum::extract::State;
use std::collections::HashMap;

use api_structs::instance::update::{Error, Parameter};
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
    tx: &mut tracing_config_helper::io_provider::Transaction,
    env: &str,
    service: &str,
) -> Result<InstanceInsertionData, Error> {
    let params = HashMap::from([
        ("env".to_string(), Parameter::String(env.to_string())),
        (
            "service".to_string(),
            Parameter::String(service.to_string()),
        ),
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
                ("env", Parameter::String(env.to_string())),
                ("name", Parameter::String(service.to_string())),
            ]);
            tx.insert("Service", map).await?
        }
        Some(id) => id.id,
    };
    let map = HashMap::from([(
        "service",
        Parameter::Uuid {
            val: service_id,
            cast_to_table: Some("Service".to_string()),
        },
    )]);
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
    let mut tx = db.transaction_start().await;
    let instance_insertion_data =
        register_instance(&mut tx, &service_id.env, &service_id.name).await?;
    tx.commit().await?;
    Ok(Json(RegistrationResponse {
        instance_id: instance_insertion_data.service_instance_id,
    }))
}
