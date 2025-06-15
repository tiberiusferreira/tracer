use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::instance::update::{ExecutionRecording, InstanceSnapshot};
use axum::Json;
use axum::extract::State;
use gel_io_recorder::{Error, Parameter, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::panic::Location;
use thiserror::Error;
use uuid::Uuid;

#[allow(unused)]
pub fn shorten_for_logging(text: &str, max_len: usize) -> String {
    // this is bytes, not chars, but close enough for debugging
    if text.len() > max_len {
        let first: String = text.chars().take(max_len / 2).collect();
        // we just got the chars in reverse order
        let last: String = text.chars().rev().take(max_len / 2).collect();
        let last = last.chars().rev().collect::<String>();
        format!("{first}\n...\n{last}")
    } else {
        text.to_string()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DbPartialExecution {
    pub id: Uuid,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub ended: bool,
}

async fn process_execution_recording(
    tx: &mut Transaction,
    instance_id: Uuid,
    recording: &ExecutionRecording,
) -> Result<(), gel_io_recorder::Error> {
    let existing_execution: Option<DbPartialExecution> = tx
        .query_optional(
            "select Execution{
      id,
      last_seen_at,
      ended
    } filter
      .service_instance = <ServiceInstance><uuid>$service_instance_id
      and .external_id = <uuid>$external_id
      ",
            HashMap::from([
                (
                    "service_instance_id".to_string(),
                    Parameter::from(instance_id),
                ),
                ("external_id".to_string(), Parameter::from(recording.id)),
            ]),
        )
        .await?;
    let _execution_id = match existing_execution {
        None => {
            let params = HashMap::from([
                (
                    "service_instance",
                    Parameter::from((instance_id, "ServiceInstance")),
                ),
                ("external_id", Parameter::from(recording.id)),
                ("started_at", Parameter::from(recording.started_at)),
                ("last_seen_at", Parameter::from(recording.last_seen_at)),
                ("ended", Parameter::from(recording.ended)),
                (
                    "replay_data",
                    Parameter::from(serde_json::to_value(&recording.replay_data).unwrap()),
                ),
            ]);
            let execution_id = tx.insert("Execution", params).await?;
            for (attr_name, attr_values) in &recording.attributes {
                #[derive(Debug, Clone, Serialize, Deserialize)]
                struct AttrName {
                    id: Uuid,
                }

                let attr_name_id: Option<AttrName> = tx
                    .query_optional(
                        "select AttributeName{
  id
} filter ._value=<str>$attr_name",
                        HashMap::from([("attr_name".to_string(), Parameter::from(attr_name))]),
                    )
                    .await?;
                let attr_name_id = match attr_name_id {
                    None => {
                        let params = HashMap::from([("_value", Parameter::from(attr_name))]);
                        let id = tx.insert("AttributeName", params).await?;
                        id
                    }
                    Some(id) => id.id,
                };
                for attr_value in attr_values {
                    #[derive(Debug, Clone, Serialize, Deserialize)]
                    struct AttrVal {
                        id: Uuid,
                    }

                    let attr_val_id: Option<AttrVal> = tx
                        .query_optional(
                            "select AttributeValue{
  id
} filter ._value=<str>$attr_val",
                            HashMap::from([("attr_val".to_string(), Parameter::from(attr_value))]),
                        )
                        .await?;
                    let attr_val_id = match attr_val_id {
                        None => {
                            let params = HashMap::from([("_value", Parameter::from(attr_value))]);
                            let id = tx.insert("AttributeValue", params).await?;
                            id
                        }
                        Some(id) => id.id,
                    };
                    let params = HashMap::from([
                        ("execution", Parameter::from((execution_id, "Execution"))),
                        (
                            "normalized_name",
                            Parameter::from((attr_name_id, "AttributeName")),
                        ),
                        (
                            "normalized_value",
                            Parameter::from((attr_val_id, "AttributeValue")),
                        ),
                    ]);
                    let _id = tx.insert("ExecutionAttribute", params).await?;
                }
            }
            execution_id
        }
        Some(existing_execution) => {
            // TODO update
            existing_execution.id
        }
    };
    Ok(())
}

#[derive(Debug, Error)]
pub enum ProcessUpdateError {
    #[error("Instance with id {id} not found at {location}")]
    InstanceNotFound {
        id: Uuid,
        location: &'static Location<'static>,
    },
    #[error("Gel Error at {location}")]
    Gel {
        #[source]
        source: gel_io_recorder::Error,
        location: &'static Location<'static>,
    },
}
impl From<gel_io_recorder::Error> for ProcessUpdateError {
    fn from(value: Error) -> Self {
        Self::Gel {
            source: value,
            location: Location::caller(),
        }
    }
}

async fn process_update(
    tx: &mut Transaction,
    instance_snapshot: &InstanceSnapshot,
) -> Result<(), ProcessUpdateError> {
    let Some(id): Option<gel_io_recorder::Id> = tx
        .query_optional(
            "select ServiceInstance filter .id=<uuid>$id",
            HashMap::from([(
                "id".to_string(),
                Parameter::from(instance_snapshot.instance_id),
            )]),
        )
        .await?
    else {
        return Err(ProcessUpdateError::InstanceNotFound {
            id: instance_snapshot.instance_id,
            location: Location::caller(),
        });
    };
    if let Some(profile) = &instance_snapshot.cpu_profile_base64 {
        let params = HashMap::from([("latest_profile_base64", Parameter::from(profile))]);
        let updated = tx.update("ServiceInstance", id.id, params).await?;
        assert!(updated);
    }
    for recording in &instance_snapshot.execution_recordings {
        process_execution_recording(&mut *tx, instance_snapshot.instance_id, recording).await?;
    }
    Ok(())
}

pub async fn handler(
    State(app_state): State<AppState>,
    instance_snapshot: Json<InstanceSnapshot>,
) -> Result<(), ApiError> {
    let io_provider = app_state.execution_io_provider;
    let instance_snapshot = instance_snapshot.0;
    let db = io_provider.database().clone();
    let mut tx = db.transaction_start().await;
    process_update(&mut tx, &instance_snapshot).await?;
    tx.commit().await?;
    Ok(())
}
