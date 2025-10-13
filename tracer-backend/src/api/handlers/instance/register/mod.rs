use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::ServiceId;
use api_structs::instance::registration::RegistrationResponse;
use axum::Json;
use axum::extract::State;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use gel_io_provider::ToParameters;
use gel_io_to_parameters::ToParameters;

struct InstanceInsertionData {
    service_instance_id: Uuid,
}


async fn get_existing_service(tx: &mut gel_io_provider::Transaction, env: String,
                              service: String) -> Result<Option<Uuid>, gel_io_provider::Error> {
    // GelGen(query, out=Id, id=de995d)
    let q = "select Service{id}
    filter .env = <str>$env and .name = <str>$service;";

    // GelGen(in, id=de995d)
    #[derive(Clone, Serialize, Deserialize, ToParameters)]
    struct Args {
        env: String,
        service: String,
    }
    // GelGen(out, id=de995d)
    #[derive(Clone, Serialize, Deserialize)]
    struct Id {
        id: Uuid,
    }

    let out: Option<Id> = tx.query_optional(q, Args {
        env,
        service,
    }.to_parameters()).await?;
    Ok(out.map(|id| id.id))
}

async fn insert_service(tx: &mut gel_io_provider::Transaction, env: String,
                        service: String) -> Result<Uuid, gel_io_provider::Error> {
    // GelGen(query, out=InsertedService, id=5c04bc)
    let insert_service_instance_query = "insert Service {
        env := <str>$env,
        name := <str>$name
    };";

    // GelGen(in, id=5c04bc)
    #[derive(Clone, Serialize, Deserialize, ToParameters)]
    struct Args {
        env: String,
        name: String,
    }
    // GelGen(out, id=5c04bc)
    #[derive(Clone, Serialize, Deserialize)]
    struct InsertedService {
        id: Uuid,
    }
    let service: InsertedService = tx.query_required_single(insert_service_instance_query, Args {
        env,
        name: service,
    }.to_parameters())
        .await?;
    Ok(service.id)
}


async fn register_instance(
    tx: &mut gel_io_provider::Transaction,
    env: &str,
    service: &str,
) -> Result<InstanceInsertionData, gel_io_provider::Error> {
    let existing_service = get_existing_service(tx, env.to_string(), service.to_string()).await?;

    let service_id = match existing_service {
        None => {
            insert_service(tx, env.to_string(), service.to_string()).await?
        }
        Some(id) => id,
    };
    // GelGen(query, out=InsertedServiceInstance, id=60a689)
    let insert_service_instance_query = "insert ServiceInstance {
        service := <Service><uuid>$service_id,
    };";

    // GelGen(in, id=60a689)
    #[derive(Clone, Serialize, Deserialize, ToParameters)]
    struct Args {
        service_id: Uuid,
    }
    // GelGen(out, id=60a689)
    #[derive(Clone, Serialize, Deserialize)]
    struct InsertedServiceInstance {
        id: Uuid,
    }

    let service_instance_id: InsertedServiceInstance = tx
        .query_required_single(insert_service_instance_query, Args {
            service_id,
        }.to_parameters(),
        )
        .await?;


    Ok(InstanceInsertionData {
        service_instance_id: service_instance_id.id,
    })
}

pub async fn handler(
    app_state: State<AppState>,
    service_id: Json<ServiceId>,
) -> Result<Json<RegistrationResponse>, ApiError> {
    let service_id = service_id.0;
    let db = app_state.execution_io_provider.clone();
    let mut tx = db.transaction_start().await?;
    let instance_insertion_data =
        register_instance(&mut tx, &service_id.env, &service_id.name).await?;
    tx.commit().await?;
    Ok(Json(RegistrationResponse {
        instance_id: instance_insertion_data.service_instance_id,
    }))
}
