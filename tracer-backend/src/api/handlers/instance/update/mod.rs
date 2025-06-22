use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::instance::update::{ExecutionRecording, InstanceSnapshot};
use axum::Json;
use axum::extract::State;
use gel_io_recorder::{Error, Parameter, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::panic::Location;
use thiserror::Error;
use tracing_config_helper::io_provider::execution_recorder::record_single_attribute;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbPartialExecution {
    pub id: Uuid,
    pub external_id: Uuid,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub ended: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbAttributeName {
    pub id: Uuid,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbAttributeValue {
    pub id: Uuid,
    pub value: String,
}

async fn process_execution_recording(
    tx: &mut Transaction,
    instance_service_info: &InstanceServiceInformation,
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
                    Parameter::from(instance_service_info.instance_id.clone()),
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
                    Parameter::from((instance_service_info.instance_id, "ServiceInstance")),
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
            let mut attributes_to_insert = recording.attributes.clone();
            attributes_to_insert.insert(
                "instance_id".to_string(),
                HashSet::from([instance_service_info.instance_id.to_string()]),
            );
            attributes_to_insert.insert(
                "service_name".to_string(),
                HashSet::from([instance_service_info.service_name.clone()]),
            );
            attributes_to_insert.insert(
                "service_env".to_string(),
                HashSet::from([instance_service_info.service_env.clone()]),
            );
            for (attr_name, attr_values) in &attributes_to_insert {
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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InstanceServiceInformation {
    service_name: String,
    service_env: String,
    instance_id: Uuid,
}

async fn process_update(
    tx: &mut Transaction,
    instance_snapshot: &InstanceSnapshot,
) -> Result<(), ProcessUpdateError> {
    let Some(instance_service_info): Option<InstanceServiceInformation> = tx
        .query_optional(
            "select ServiceInstance{
  service_name := .service.name,
  service_env := .service.env,
  instance_id := .id,
} filter .id=<uuid>$instance_id",
            HashMap::from([(
                "instance_id".to_string(),
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
        let updated = tx
            .update("ServiceInstance", instance_service_info.instance_id, params)
            .await?;
        assert!(updated);
    }
    process_recordings(tx, &instance_snapshot.execution_recordings).await?;
    for recording in &instance_snapshot.execution_recordings {
        process_execution_recording(&mut *tx, &instance_service_info, recording).await?;
    }
    Ok(())
}

async fn get_matching_db_execution(
    tx: &mut Transaction,
    external_execution_ids: &[Uuid],
) -> Result<Vec<DbPartialExecution>, gel_io_recorder::Error> {
    let existing_execution: Vec<DbPartialExecution> = tx
        .query_multiple(
            "with
  execution_ids_json := <json>$execution_ids_json,
  execution_ids_set :=
  (
    for item in json_array_unpack(execution_ids_json) union (
      select <uuid>item
    )
  )
  select Execution{
      id,
      external_id,
      last_seen_at,
      ended
    } filter
      .external_id = execution_ids_set
      ",
            HashMap::from([(
                "execution_ids_json".to_string(),
                Parameter::from(
                    serde_json::to_value(&external_execution_ids).expect("uuids are serializable"),
                ),
            )]),
        )
        .await?;
    Ok(existing_execution)
}

async fn get_db_attribute_names(
    tx: &mut Transaction,
    attribute_name: &HashSet<String>,
) -> Result<Vec<DbAttributeName>, gel_io_recorder::Error> {
    let existing_attribute_names: Vec<DbAttributeName> = tx
        .query_multiple(
            "with
  attribute_names_json := <json>$attribute_names_json,
  attribute_names_set :=
  (
    for item in json_array_unpack(attribute_names_json) union (
      select <str>item
    )
  )
select AttributeName{
  id,
  value := ._value
} filter ._value in attribute_names_set;
      ",
            HashMap::from([(
                "attribute_names_json".to_string(),
                Parameter::from(
                    serde_json::to_value(&attribute_name).expect("names are serializable"),
                ),
            )]),
        )
        .await?;
    Ok(existing_attribute_names)
}

async fn get_db_attribute_values(
    tx: &mut Transaction,
    attribute_values: &HashSet<String>,
) -> Result<Vec<DbAttributeValue>, gel_io_recorder::Error> {
    let existing_attribute_values: Vec<DbAttributeValue> = tx
        .query_multiple(
            "with
  attribute_names_json := <json>$attribute_values_json,
  attribute_names_set :=
  (
    for item in json_array_unpack(attribute_names_json) union (
      select <str>item
    )
  )
select AttributeValue{
  id,
  value := ._value
} filter ._value in attribute_names_set;
      ",
            HashMap::from([(
                "attribute_values_json".to_string(),
                Parameter::from(
                    serde_json::to_value(&attribute_values).expect("values are serializable"),
                ),
            )]),
        )
        .await?;

    Ok(existing_attribute_values)
}

async fn process_recordings(
    tx: &mut Transaction,
    recording: &[ExecutionRecording],
) -> Result<(), gel_io_recorder::Error> {
    let external_execution_ids = recording.iter().map(|r| r.id).collect::<Vec<_>>();
    let existing_executions = get_matching_db_execution(&mut *tx, &external_execution_ids).await?;
    let attribute_names = recording
        .iter()
        .flat_map(|r| r.attributes.keys())
        .cloned()
        .collect::<HashSet<_>>();
    let attribute_value = recording
        .iter()
        .flat_map(|r| r.attributes.values().flatten())
        .cloned()
        .collect::<HashSet<_>>();

    let mut db_attribute_names = get_db_attribute_names(&mut *tx, &attribute_names).await?;
    let db_names: HashSet<String> = db_attribute_names.iter().map(|e| e.value.clone()).collect();
    let db_attribute_values = get_db_attribute_values(&mut *tx, &attribute_value).await?;
    let db_attributes: HashSet<String> = db_attribute_values
        .iter()
        .map(|e| e.value.clone())
        .collect();
    let missing_attribute_names: Vec<String> =
        attribute_names.difference(&db_names).cloned().collect();
    let mut name_params = vec![];
    for name in &missing_attribute_names {
        let mut single_params = HashMap::new();
        single_params.insert("_value".to_string(), Parameter::from(name));
        name_params.push(single_params);
    }
    let inserted_attr_names_ids = tx.bulk_insert("AttributeName", name_params).await?;
    for (idx, missing_name) in missing_attribute_names.iter().enumerate() {
        db_attribute_names.push(DbAttributeName {
            id: *inserted_attr_names_ids
                .get(idx)
                .expect("inserted value to exist"),
            value: missing_name.clone(),
        })
    }
    let mut value_params = vec![];
    let missing_attribute_values: Vec<String> = attribute_value
        .difference(&db_attributes)
        .cloned()
        .collect();
    for name in &missing_attribute_values {
        let mut single_params = HashMap::new();
        single_params.insert("_value".to_string(), Parameter::from(name));
        value_params.push(single_params);
    }
    let inserted_attr_val_ids = tx.bulk_insert("AttributeValue", value_params).await?;
    for (idx, missing_val) in missing_attribute_values.iter().enumerate() {
        db_attribute_names.push(DbAttributeName {
            id: *inserted_attr_val_ids
                .get(idx)
                .expect("inserted value to exist"),
            value: missing_val.clone(),
        })
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
    let mut tx = db.transaction_start().await?;
    process_update(&mut tx, &instance_snapshot).await?;
    tx.commit().await?;
    Ok(())
}
