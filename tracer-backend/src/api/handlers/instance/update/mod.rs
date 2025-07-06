use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::instance::update::{ExecutionRecording, InstanceSnapshot, ReplayDataFragment};
use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
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
    pub size_bytes: i32,
    pub attributes: Vec<DbAttribute>,
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
    instance_snapshot: &mut InstanceSnapshot,
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
    for exec in &mut instance_snapshot.execution_recordings {
        exec.attributes.insert(
            "service_env".to_string(),
            HashSet::from([instance_service_info.service_env.to_string()]),
        );
        exec.attributes.insert(
            "service_name".to_string(),
            HashSet::from([instance_service_info.service_name.to_string()]),
        );
        exec.attributes.insert(
            "instance_id".to_string(),
            HashSet::from([instance_service_info.instance_id.to_string()]),
        );
    }
    store_new_recording_data(
        tx,
        &instance_service_info,
        &instance_snapshot.execution_recordings,
    )
    .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DbAttribute {
    name_uuid: Uuid,
    value_uuid: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutionAttributeToInsert {
    execution_external_id: Uuid,
    name_uuid: Uuid,
    value_uuid: Uuid,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutionToGet {
    external_id: Uuid,
    attributes: Vec<DbAttribute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbExecutionToGetResponse {
    id: Uuid,
    ended: bool,
    external_id: Uuid,
    attributes: Vec<DbAttribute>,
}

async fn get_existing_executions(
    tx: &mut Transaction,
    executions_to_get: &[ExecutionToGet],
) -> Result<Vec<DbPartialExecution>, gel_io_recorder::Error> {
    let existing_execution: Vec<DbPartialExecution> = tx
        .query_multiple(
            "with
  executions_to_get := <json>$executions_to_get,
for item in json_array_unpack(executions_to_get) union (
  select Execution{
    id,
    external_id,
    size_bytes,
    ended,
    attributes := assert_distinct((
      for single_attribute in json_array_unpack(item['attributes']) union (
        select (.<execution[is ExecutionAttribute]{
          name_uuid := .normalized_name.id,
          value_uuid := .normalized_value.id,
        }
        )
        filter .normalized_name.id=<uuid>single_attribute['name_uuid']
          and .normalized_value.id=<uuid>single_attribute['value_uuid']
      )
    ))
  }
  filter .external_id=<uuid>item['external_id']
)",
            HashMap::from([(
                "executions_to_get".to_string(),
                Parameter::from(
                    serde_json::to_value(executions_to_get).expect("uuids are serializable"),
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

async fn map_attribute_names_to_db_inserting_missing(
    tx: &mut Transaction,
    attribute_names_used_by_recordings: HashSet<String>,
) -> Result<HashMap<String, Uuid>, gel_io_recorder::Error> {
    let attribute_names_already_in_db =
        get_db_attribute_names(&mut *tx, &attribute_names_used_by_recordings).await?;
    let mut attribute_name_to_db_id: HashMap<String, Uuid> = HashMap::new();
    let mut missing_attr_names_in_db = Vec::new();
    for name in attribute_names_used_by_recordings {
        let maybe_existing_id = attribute_names_already_in_db
            .iter()
            .find_map(|e| if e.value == name { Some(e.id) } else { None });
        match maybe_existing_id {
            None => {
                missing_attr_names_in_db.push(name);
            }
            Some(existing_id) => {
                attribute_name_to_db_id.insert(name, existing_id);
            }
        }
    }
    missing_attr_names_in_db.sort();
    let ordered_missing_attr_names_in_db = missing_attr_names_in_db;
    let mut name_params = vec![];
    for name in &ordered_missing_attr_names_in_db {
        let mut single_params = HashMap::new();
        single_params.insert("_value".to_string(), Parameter::from(name));
        name_params.push(single_params);
    }
    let inserted_attr_names_ids = tx
        .bulk_insert("AttributeName", name_params, "_value")
        .await?;
    assert_eq!(
        inserted_attr_names_ids.len(),
        ordered_missing_attr_names_in_db.len()
    );
    for (inserted_attr_name, inserted_uuid) in ordered_missing_attr_names_in_db
        .iter()
        .zip(inserted_attr_names_ids.iter())
    {
        attribute_name_to_db_id.insert(inserted_attr_name.clone(), *inserted_uuid);
    }
    Ok(attribute_name_to_db_id)
}
async fn map_attribute_values_to_db_inserting_missing(
    tx: &mut Transaction,
    attribute_values_used_by_recordings: HashSet<String>,
) -> Result<HashMap<String, Uuid>, gel_io_recorder::Error> {
    let attribute_values_already_in_db =
        get_db_attribute_values(&mut *tx, &attribute_values_used_by_recordings).await?;
    let mut attribute_value_to_db_id: HashMap<String, Uuid> = HashMap::new();
    let mut missing_attr_values_in_db = Vec::new();
    for name in attribute_values_used_by_recordings {
        let maybe_existing_id = attribute_values_already_in_db
            .iter()
            .find_map(|e| if e.value == name { Some(e.id) } else { None });
        match maybe_existing_id {
            None => {
                missing_attr_values_in_db.push(name);
            }
            Some(existing_id) => {
                attribute_value_to_db_id.insert(name, existing_id);
            }
        }
    }
    missing_attr_values_in_db.sort();
    let ordered_missing_attr_names_in_db = missing_attr_values_in_db;
    let mut values_params = vec![];
    for name in &ordered_missing_attr_names_in_db {
        let mut single_params = HashMap::new();
        single_params.insert("_value".to_string(), Parameter::from(name));
        values_params.push(single_params);
    }
    let inserted_attr_values_ids = tx
        .bulk_insert("AttributeValue", values_params, "_value")
        .await?;
    assert_eq!(
        inserted_attr_values_ids.len(),
        ordered_missing_attr_names_in_db.len()
    );
    for (inserted_attr_value, inserted_uuid) in ordered_missing_attr_names_in_db
        .iter()
        .zip(inserted_attr_values_ids.iter())
    {
        attribute_value_to_db_id.insert(inserted_attr_value.clone(), *inserted_uuid);
    }
    Ok(attribute_value_to_db_id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MappedAttributeKeyValue {
    attribute_name_id: Uuid,
    attribute_name: String,
    attribute_value_id: Uuid,
    attribute_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbExecutionAttribute {
    attribute_name_id: Uuid,
    attribute_value_id: Uuid,
}
async fn get_existing_execution_attribute_kv(
    tx: &mut Transaction,
    mapped_attribute_kv: &[DbAttribute],
    execution_id: Uuid,
) -> Result<Vec<DbExecutionAttribute>, gel_io_recorder::Error> {
    let existing_attribute_values: Vec<DbExecutionAttribute> = tx
        .query_multiple(
            "with
  attribute_names_json := <json>$attribute_key_values_json,
for item in json_array_unpack(attribute_names_json) union (
  select ExecutionAttribute{
    normalized_name_id := .normalized_name.id,
    normalized_value_id := .normalized_value.id,
  } filter .execution.id=<uuid>$execution_id
    and .normalized_name.id=<uuid>item['name_uuid']
    and .normalized_value.id=<uuid>item['value_uuid']

)
      ",
            HashMap::from([
                (
                    "attribute_key_values_json".to_string(),
                    Parameter::from(
                        serde_json::to_value(&mapped_attribute_kv)
                            .expect("values are serializable"),
                    ),
                ),
                ("execution_id".to_string(), Parameter::from(execution_id)),
            ]),
        )
        .await?;

    Ok(existing_attribute_values)
}

// async fn insert_replay_data_fragment(
//     tx: &mut Transaction,
//     execution_id: Uuid,
//     replay_fragment: ReplayDataFragment,
// ) -> Result<(), gel_io_recorder::Error> {
// }
async fn insert_new_attributes(
    tx: &mut Transaction,
    execution_id: Uuid,
    mapped_attribute_kv: Vec<DbAttribute>,
) -> Result<(), gel_io_recorder::Error> {
    let attribute_values_already_in_db =
        get_existing_execution_attribute_kv(&mut *tx, &mapped_attribute_kv, execution_id).await?;
    let missing_attributes = mapped_attribute_kv
        .iter()
        .filter(|new_attr_kv| {
            attribute_values_already_in_db
                .iter()
                .find(|db_attr_kv| {
                    db_attr_kv.attribute_name_id == new_attr_kv.name_uuid
                        && db_attr_kv.attribute_value_id == new_attr_kv.value_uuid
                })
                .is_none()
        })
        .collect::<Vec<_>>();
    let mut all_params = Vec::new();
    for missing_attr in missing_attributes {
        let mut params = HashMap::new();
        params.insert(
            "execution".to_string(),
            Parameter::from((execution_id, "Execution")),
        );
        params.insert(
            "normalized_name".to_string(),
            Parameter::from((missing_attr.name_uuid, "AttributeName")),
        );
        params.insert(
            "normalized_value".to_string(),
            Parameter::from((missing_attr.value_uuid, "AttributeValue")),
        );
        all_params.push(params);
    }
    tx.bulk_insert("ExecutionAttribute", all_params, "id")
        .await?;
    Ok(())
}

async fn insert_new_execution(
    tx: &mut Transaction,
    service_instance_id: Uuid,
    external_id: Uuid,
    started_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    ended: bool,
) -> Result<Uuid, gel_io_recorder::Error> {
    let params = HashMap::from([
        (
            "service_instance",
            Parameter::from((service_instance_id, "ServiceInstance")),
        ),
        ("external_id", Parameter::from(external_id)),
        ("started_at", Parameter::from(started_at)),
        ("last_seen_at", Parameter::from(last_seen_at)),
        ("ended", Parameter::from(ended)),
    ]);
    tx.insert("Execution", params).await
}

struct ExecutionHeaderToInsert {
    service_instance_id: Uuid,
    external_id: Uuid,
    started_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    size_bytes: i32,
    ended: bool,
}

struct ExecutionHeaderToUpdate {
    id: Uuid,
    last_seen_at: DateTime<Utc>,
    size_bytes: i32,
    ended: bool,
}
struct ReplayDataToInsert {
    execution_external_id: Uuid,
    replay_data: serde_json::Value,
}
async fn store_new_recording_data(
    tx: &mut Transaction,
    instance_service_info: &InstanceServiceInformation,
    recording: &[ExecutionRecording],
) -> Result<(), gel_io_recorder::Error> {
    // We can have multiple values for the same key, but we can't and don't want to store the same key-value pair twice.
    // To prevent this, we check if the key-value already exists before inserting it.
    // We expect most of the executions to be brand new.
    let attribute_names_used_by_recordings = recording
        .iter()
        .flat_map(|r| r.attributes.keys())
        .cloned()
        .collect::<HashSet<_>>();
    let attribute_name_to_db_id =
        map_attribute_names_to_db_inserting_missing(&mut *tx, attribute_names_used_by_recordings)
            .await?;
    let attribute_values_used_by_recordings = recording
        .iter()
        .flat_map(|r| r.attributes.values().flatten())
        .cloned()
        .collect::<HashSet<_>>();
    let attribute_value_to_db_id =
        map_attribute_values_to_db_inserting_missing(&mut *tx, attribute_values_used_by_recordings)
            .await?;
    let mut executions_and_attributes_to_get = vec![];
    for single_rec in recording {
        let attributes = single_rec
            .attributes
            .iter()
            .flat_map(|(k, v)| {
                let mut db_attributes = vec![];
                for value in v {
                    db_attributes.push(DbAttribute {
                        name_uuid: *attribute_name_to_db_id.get(k).unwrap(),
                        value_uuid: *attribute_value_to_db_id.get(value).unwrap(),
                    })
                }
                db_attributes
            })
            .collect();
        executions_and_attributes_to_get.push(ExecutionToGet {
            external_id: single_rec.id,
            attributes,
        });
    }
    let existing_executions =
        get_existing_executions(&mut *tx, &executions_and_attributes_to_get).await?;
    let mut executions_headers_to_insert = vec![];
    let mut executions_headers_to_update = vec![];
    let mut executions_attributes_to_insert = vec![];
    let mut executions_replay_fragment_to_insert = vec![];
    let mut external_id_to_db_id = HashMap::new();
    for single_rec in recording {
        let matching_execution = existing_executions
            .iter()
            .find(|e| e.external_id == single_rec.id);
        match matching_execution {
            None => {
                let replay_data_json_value =
                    serde_json::to_value(&single_rec.replay_data_fragment).unwrap();
                let json_size_bytes = serde_json::to_string(&replay_data_json_value)
                    .unwrap()
                    .len();
                executions_headers_to_insert.push(ExecutionHeaderToInsert {
                    service_instance_id: instance_service_info.instance_id,
                    external_id: single_rec.id,
                    started_at: single_rec.started_at,
                    last_seen_at: single_rec.last_seen_at,
                    size_bytes: json_size_bytes as i32,
                    ended: single_rec.ended,
                });
                let attributes: Vec<DbAttribute> = single_rec
                    .attributes
                    .iter()
                    .flat_map(|(k, v)| {
                        let mut db_attributes = vec![];
                        for value in v {
                            db_attributes.push(DbAttribute {
                                name_uuid: *attribute_name_to_db_id.get(k).unwrap(),
                                value_uuid: *attribute_value_to_db_id.get(value).unwrap(),
                            })
                        }
                        db_attributes
                    })
                    .collect();
                for attribute in attributes {
                    executions_attributes_to_insert.push(ExecutionAttributeToInsert {
                        execution_external_id: single_rec.id,
                        name_uuid: attribute.name_uuid,
                        value_uuid: attribute.value_uuid,
                    });
                }
                executions_replay_fragment_to_insert.push(ReplayDataToInsert {
                    execution_external_id: single_rec.id,
                    replay_data: replay_data_json_value,
                });
            }
            Some(existing_execution) => {
                external_id_to_db_id.insert(single_rec.id, existing_execution.id);
                record_single_attribute(
                    "updates_existing_execution".to_string(),
                    "true".to_string(),
                );
                let replay_data_json_value =
                    serde_json::to_value(&single_rec.replay_data_fragment).unwrap();
                let json_size_bytes = serde_json::to_string(&replay_data_json_value)
                    .unwrap()
                    .len();
                executions_headers_to_update.push(ExecutionHeaderToUpdate {
                    id: existing_execution.id,
                    last_seen_at: single_rec.last_seen_at,
                    size_bytes: existing_execution.size_bytes + json_size_bytes as i32,
                    ended: single_rec.ended,
                });
                let attributes: Vec<DbAttribute> = single_rec
                    .attributes
                    .iter()
                    .flat_map(|(k, v)| {
                        let mut db_attributes = vec![];
                        for value in v {
                            db_attributes.push(DbAttribute {
                                name_uuid: *attribute_name_to_db_id.get(k).unwrap(),
                                value_uuid: *attribute_value_to_db_id.get(value).unwrap(),
                            })
                        }
                        db_attributes
                    })
                    .collect();
                for attribute in attributes {
                    if existing_execution.attributes.contains(&attribute) {
                        continue;
                    }
                    executions_attributes_to_insert.push(ExecutionAttributeToInsert {
                        execution_external_id: single_rec.id,
                        name_uuid: attribute.name_uuid,
                        value_uuid: attribute.value_uuid,
                    });
                }
                executions_replay_fragment_to_insert.push(ReplayDataToInsert {
                    execution_external_id: single_rec.id,
                    replay_data: replay_data_json_value,
                });
            }
        }
    }
    let mut multi_exec_params = vec![];
    for e in &executions_headers_to_insert {
        let mut params = HashMap::new();
        params.insert(
            "service_instance".to_string(),
            Parameter::from((e.service_instance_id, "ServiceInstance")),
        );
        params.insert("external_id".to_string(), Parameter::from(e.external_id));
        params.insert("size_bytes".to_string(), Parameter::from(e.size_bytes));
        params.insert("started_at".to_string(), Parameter::from(e.started_at));
        params.insert("last_seen_at".to_string(), Parameter::from(e.last_seen_at));
        params.insert("ended".to_string(), Parameter::from(e.ended));
        multi_exec_params.push(params);
    }
    for to_update in executions_headers_to_update {
        let mut params = HashMap::new();
        params.insert(
            "size_bytes".to_string(),
            Parameter::from(to_update.size_bytes),
        );
        params.insert(
            "last_seen_at".to_string(),
            Parameter::from(to_update.last_seen_at),
        );
        params.insert("ended".to_string(), Parameter::from(to_update.ended));

        let was_updated = tx.update("Execution", to_update.id, params).await?;
        assert!(was_updated);
    }
    let ids = tx
        .bulk_insert("Execution", multi_exec_params, "external_id")
        .await?;
    let mut external_ids = executions_headers_to_insert
        .iter()
        .map(|e| e.external_id)
        .collect::<Vec<_>>();
    external_ids.sort();
    for (idx, external_id) in external_ids.iter().enumerate() {
        external_id_to_db_id.insert(*external_id, *ids.get(idx).unwrap());
    }

    let mut all_params = Vec::new();
    for missing_attr in &executions_attributes_to_insert {
        let mut params = HashMap::new();
        let execution_db_id = *external_id_to_db_id
            .get(&missing_attr.execution_external_id)
            .unwrap();
        params.insert(
            "execution".to_string(),
            Parameter::from((execution_db_id, "Execution")),
        );
        params.insert(
            "normalized_name".to_string(),
            Parameter::from((missing_attr.name_uuid, "AttributeName")),
        );
        params.insert(
            "normalized_value".to_string(),
            Parameter::from((missing_attr.value_uuid, "AttributeValue")),
        );
        all_params.push(params);
    }
    tx.bulk_insert("ExecutionAttribute", all_params, "id")
        .await?;

    let mut all_params = Vec::new();
    for replay_fragment in executions_replay_fragment_to_insert {
        let mut params = HashMap::new();
        let execution_db_id = *external_id_to_db_id
            .get(&replay_fragment.execution_external_id)
            .unwrap();
        params.insert(
            "execution".to_string(),
            Parameter::from((execution_db_id, "Execution")),
        );
        params.insert(
            "replay_data".to_string(),
            Parameter::from(replay_fragment.replay_data),
        );
        all_params.push(params);
    }
    tx.bulk_insert("ReplayFragment", all_params, "id").await?;

    Ok(())
}

pub async fn handler(
    State(app_state): State<AppState>,
    instance_snapshot: Json<InstanceSnapshot>,
) -> Result<(), ApiError> {
    let io_provider = app_state.execution_io_provider;
    let mut instance_snapshot = instance_snapshot.0;
    let db = io_provider.database().clone();
    let mut tx = db.transaction_start().await?;
    process_update(&mut tx, &mut instance_snapshot).await?;
    tx.commit().await?;
    Ok(())
}
