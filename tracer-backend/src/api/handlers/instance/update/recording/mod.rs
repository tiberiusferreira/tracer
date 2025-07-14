use crate::api::handlers::instance::update::{GelError, InstanceServiceInformation};
use api_structs::instance::update::ExecutionRecording;
use chrono::{DateTime, Utc};
use gel_io_recorder::{Parameter, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing_config_helper::io_provider::execution_recorder::record_single_attribute;
use uuid::Uuid;
mod attribute;

pub async fn store_new_recording_data(
    tx: &mut Transaction,
    instance_service_info: &InstanceServiceInformation,
    recording: &[ExecutionRecording],
) -> Result<(), GelError> {
    // We can have multiple values for the same key, but we can't and don't want to store the same key-value pair twice.
    // To prevent this, we check if the key-value already exists before inserting it.
    // We expect most of the executions to be brand new.
    let used_attributes_data = attribute::get_all_used_attributes(recording);
    let attribute_name_to_db_id = attribute::map_attribute_names_to_db_inserting_missing(
        &mut *tx,
        used_attributes_data.names,
    )
    .await?;
    let attribute_value_to_db_id = attribute::map_attribute_values_to_db_inserting_missing(
        &mut *tx,
        used_attributes_data.values,
    )
    .await?;
    let mut executions_and_attributes_to_get = vec![];
    for single_rec in recording {
        let attributes = single_rec
            .attributes
            .iter()
            .flat_map(|(k, v)| {
                let mut db_attributes = vec![];
                for value in v {
                    db_attributes.push(DbAttributeNameAndValue {
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
                let attributes: Vec<DbAttributeNameAndValue> = single_rec
                    .attributes
                    .iter()
                    .flat_map(|(k, v)| {
                        let mut db_attributes = vec![];
                        for value in v {
                            db_attributes.push(DbAttributeNameAndValue {
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
                let attributes: Vec<DbAttributeNameAndValue> = single_rec
                    .attributes
                    .iter()
                    .flat_map(|(k, v)| {
                        let mut db_attributes = vec![];
                        for value in v {
                            db_attributes.push(DbAttributeNameAndValue {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbPartialExecution {
    pub id: Uuid,
    pub external_id: Uuid,
    pub size_bytes: i32,
    pub attributes: Vec<DbAttributeNameAndValue>,
    pub ended: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbAttribute {
    pub id: Uuid,
    pub value: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbAttributeNameAndValue {
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
    attributes: Vec<DbAttributeNameAndValue>,
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
