use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::instance::update::InstanceSnapshot;
use axum::Json;
use axum::extract::State;
use gel_io_recorder::{Error, Transaction};
use serde::{Deserialize, Serialize};
use std::panic::Location;
use thiserror::Error;
use uuid::Uuid;
mod instance;
mod recording;

pub async fn handler(
    State(app_state): State<AppState>,
    instance_snapshot: Json<InstanceSnapshot>,
) -> Result<(), ApiError> {
    // coordinator
    let mut instance_snapshot = instance_snapshot.0;
    let mut tx = app_state.execution_io_provider.transaction_start().await?;
    println!("Running update");
    process_update(&mut tx, &mut instance_snapshot).await?;
    println!("Running commit");
    tx.commit().await?;
    Ok(())
}

async fn process_update(
    tx: &mut Transaction,
    instance_snapshot: &mut InstanceSnapshot,
) -> Result<(), ProcessUpdateError> {
    let instance_service_info =
        instance::get_instance_service_information(&mut *tx, instance_snapshot.instance_id)
            .await?
            .ok_or_else(|| ProcessUpdateError::InstanceNotFound {
                id: instance_snapshot.instance_id,
                location: Location::caller(),
            })?;
    println!("{:#?}", instance_service_info);
    if let Some(cpu_profile_base64) = &instance_snapshot.cpu_profile_base64 {
        instance::update_instance_profile(
            &mut *tx,
            instance_snapshot.instance_id,
            cpu_profile_base64,
        )
            .await?;
    }
    instance::add_instance_attributes(
        &mut instance_snapshot.execution_recordings,
        &instance_service_info.service_env,
        &instance_service_info.service_name,
        instance_service_info.instance_id,
    );
    recording::store_new_recording_data(
        tx,
        &instance_service_info,
        &instance_snapshot.execution_recordings,
    )
        .await?;
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InstanceServiceInformation {
    service_name: String,
    service_env: String,
    instance_id: Uuid,
}

#[derive(Debug, Error)]
pub enum ProcessUpdateError {
    #[error("Instance with id {id} not found at {location}")]
    InstanceNotFound {
        id: Uuid,
        location: &'static Location<'static>,
    },
    #[error(transparent)]
    Gel(#[from] GelError),
}

#[derive(Debug, Error)]
#[error("Gel Error at {location}")]
pub struct GelError {
    #[source]
    source: gel_io_recorder::Error,
    location: &'static Location<'static>,
}

impl From<gel_io_recorder::Error> for GelError {
    fn from(value: Error) -> Self {
        GelError {
            source: value,
            location: Location::caller(),
        }
    }
}

impl From<gel_io_recorder::Error> for ProcessUpdateError {
    fn from(value: Error) -> Self {
        Self::Gel(GelError {
            source: value,
            location: Location::caller(),
        })
    }
}
