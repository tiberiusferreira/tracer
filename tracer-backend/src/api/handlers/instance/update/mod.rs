use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::instance::update::{ExecutionRecording, InstanceSnapshot};
use axum::Json;
use axum::extract::State;
use function_timer::time;
use gel_io_recorder::{Parameter, Transaction2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing_config_helper::io_provider::execution_recorder::function_instrumentation::instrument_function_within_task;
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

#[time]
async fn process_execution_recording(
    tx: &mut Transaction2,
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
                    Parameter::Uuid {
                        val: instance_id,
                        cast_to_table: None,
                    },
                ),
                (
                    "external_id".to_string(),
                    Parameter::Uuid {
                        val: recording.id,
                        cast_to_table: None,
                    },
                ),
            ]),
        )
        .await?;
    let _execution_id = match existing_execution {
        None => {
            let params = HashMap::from([
                (
                    "service_instance",
                    Parameter::Uuid {
                        val: instance_id,
                        cast_to_table: Some("ServiceInstance".to_string()),
                    },
                ),
                (
                    "external_id",
                    Parameter::Uuid {
                        val: recording.id,
                        cast_to_table: None,
                    },
                ),
                ("started_at", Parameter::Datetime(recording.started_at)),
                ("last_seen_at", Parameter::Datetime(recording.last_seen_at)),
                ("ended", Parameter::Bool(recording.ended)),
                (
                    "replay_data",
                    Parameter::Json(serde_json::to_value(&recording.replay_data).unwrap()),
                ),
                (
                    "executed_functions",
                    Parameter::Json(serde_json::to_value(&recording.executed_functions).unwrap()),
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
                        HashMap::from([(
                            "attr_name".to_string(),
                            Parameter::String(attr_name.clone()),
                        )]),
                    )
                    .await?;
                let attr_name_id = match attr_name_id {
                    None => {
                        let params =
                            HashMap::from([("_value", Parameter::String(attr_name.to_string()))]);
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
                            HashMap::from([(
                                "attr_val".to_string(),
                                Parameter::String(attr_value.clone()),
                            )]),
                        )
                        .await?;
                    let attr_val_id = match attr_val_id {
                        None => {
                            let params = HashMap::from([(
                                "_value",
                                Parameter::String(attr_value.to_string()),
                            )]);
                            let id = tx.insert("AttributeValue", params).await?;
                            id
                        }
                        Some(id) => id.id,
                    };
                    let params = HashMap::from([
                        (
                            "execution",
                            Parameter::Uuid {
                                val: execution_id,
                                cast_to_table: Some("Execution".to_string()),
                            },
                        ),
                        (
                            "normalized_name",
                            Parameter::Uuid {
                                val: attr_name_id,
                                cast_to_table: Some("AttributeName".to_string()),
                            },
                        ),
                        (
                            "normalized_value",
                            Parameter::Uuid {
                                val: attr_val_id,
                                cast_to_table: Some("AttributeValue".to_string()),
                            },
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
#[time]
async fn process_update(
    tx: &mut Transaction2,
    instance_snapshot: &InstanceSnapshot,
) -> Result<(), gel_io_recorder::Error> {
    for recording in &instance_snapshot.execution_recordings {
        process_execution_recording(&mut *tx, instance_snapshot.instance_id, recording).await?;
    }
    Ok(())
}

#[time]
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
