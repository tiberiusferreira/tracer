use crate::api::handlers::instance::update::GelError;
use crate::api::handlers::instance::update::recording::DbAttribute;
use api_structs::instance::update::ExecutionRecordingSnapshot;
use gel_io_recorder::{Parameter, Transaction};
use std::collections::{HashMap, HashSet};
use indexmap::IndexMap;
use uuid::Uuid;

pub struct FlattenedAttributesData {
    pub names: HashSet<String>,
    pub values: HashSet<String>,
}
pub fn get_all_used_attributes(recording: &[ExecutionRecordingSnapshot]) -> FlattenedAttributesData {
    let attribute_names = recording
        .iter()
        .flat_map(|r| r.attributes.keys())
        .cloned()
        .collect::<HashSet<_>>();
    let attribute_values = recording
        .iter()
        .flat_map(|r| r.attributes.values().flatten())
        .cloned()
        .collect::<HashSet<_>>();
    FlattenedAttributesData {
        names: attribute_names,
        values: attribute_values,
    }
}

struct AttributesClassificationResult {
    attribute_to_db_id: HashMap<String, Uuid>,
    sorted_attributes_not_in_db: Vec<String>,
}
fn classify_attributes(
    all_attributes: HashSet<String>,
    attributes_in_db: Vec<DbAttribute>,
) -> AttributesClassificationResult {
    let mut attribute_name_to_db_id: HashMap<String, Uuid> = HashMap::new();
    let mut attributes_names_not_in_db = Vec::new();
    for name in all_attributes {
        let maybe_existing_id = attributes_in_db
            .iter()
            .find_map(|e| if e.value == name { Some(e.id) } else { None });
        match maybe_existing_id {
            None => {
                attributes_names_not_in_db.push(name);
            }
            Some(existing_id) => {
                attribute_name_to_db_id.insert(name, existing_id);
            }
        }
    }
    attributes_names_not_in_db.sort();
    let sorted_attributes_names_not_in_db = attributes_names_not_in_db;
    AttributesClassificationResult {
        attribute_to_db_id: attribute_name_to_db_id,
        sorted_attributes_not_in_db: sorted_attributes_names_not_in_db,
    }
}

pub async fn map_attribute_names_to_db_inserting_missing(
    tx: &mut Transaction,
    attribute_names_used_by_recordings: HashSet<String>,
) -> Result<HashMap<String, Uuid>, GelError> {
    let attribute_names_already_in_db =
        get_db_attribute_names(&mut *tx, &attribute_names_used_by_recordings).await?;
    let classification_result = classify_attributes(
        attribute_names_used_by_recordings,
        attribute_names_already_in_db,
    );
    let sorted_attributes_names_not_in_db = classification_result.sorted_attributes_not_in_db;

    let inserted_attr_names_ids =
        insert_attribute_names_into_db(tx, &sorted_attributes_names_not_in_db).await?;
    assert_eq!(
        sorted_attributes_names_not_in_db.len(),
        inserted_attr_names_ids.len()
    );
    let mut attribute_name_to_db_id = classification_result.attribute_to_db_id;
    for (inserted_attr_name, inserted_uuid) in sorted_attributes_names_not_in_db
        .iter()
        .zip(inserted_attr_names_ids.iter())
    {
        attribute_name_to_db_id.insert(inserted_attr_name.clone(), *inserted_uuid);
    }
    Ok(attribute_name_to_db_id)
}

async fn get_db_attribute_values(
    tx: &mut Transaction,
    attribute_values: &HashSet<String>,
) -> Result<Vec<DbAttribute>, gel_io_recorder::Error> {
    let mut attribute_values = attribute_values.into_iter().collect::<Vec<_>>();
    attribute_values.sort();
    let existing_attribute_values: Vec<DbAttribute> = tx
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
            IndexMap::from([(
                "attribute_values_json".to_string(),
                Parameter::from(
                    serde_json::to_value(&attribute_values).expect("values are serializable"),
                ),
            )]),
        )
        .await?;

    Ok(existing_attribute_values)
}
pub async fn map_attribute_values_to_db_inserting_missing(
    tx: &mut Transaction,
    attribute_values_used_by_recordings: HashSet<String>,
) -> Result<HashMap<String, Uuid>, GelError> {
    let attribute_values_already_in_db =
        get_db_attribute_values(&mut *tx, &attribute_values_used_by_recordings).await?;
    let classification_result = classify_attributes(
        attribute_values_used_by_recordings,
        attribute_values_already_in_db,
    );
    let sorted_attributes_vals_not_in_db = classification_result.sorted_attributes_not_in_db;

    let inserted_attr_vals_ids =
        insert_attribute_vals_into_db(tx, &sorted_attributes_vals_not_in_db).await?;
    assert_eq!(
        sorted_attributes_vals_not_in_db.len(),
        inserted_attr_vals_ids.len()
    );
    let mut attribute_name_to_db_id = classification_result.attribute_to_db_id;
    for (inserted_attr_val, inserted_uuid) in sorted_attributes_vals_not_in_db
        .iter()
        .zip(inserted_attr_vals_ids.iter())
    {
        attribute_name_to_db_id.insert(inserted_attr_val.clone(), *inserted_uuid);
    }
    Ok(attribute_name_to_db_id)
}

async fn insert_attribute_vals_into_db(
    tx: &mut Transaction,
    order_attributes_names_not_in_db: &[String],
) -> Result<Vec<Uuid>, GelError> {
    let mut name_params = vec![];
    for name in order_attributes_names_not_in_db {
        let mut single_params = IndexMap::new();
        single_params.insert("_value".to_string(), Parameter::from(name));
        name_params.push(single_params);
    }
    let inserted_attr_names_ids = tx
        .bulk_insert("AttributeValue", name_params, "_value")
        .await?;
    Ok(inserted_attr_names_ids)
}
async fn insert_attribute_names_into_db(
    tx: &mut Transaction,
    order_attributes_names_not_in_db: &[String],
) -> Result<Vec<Uuid>, GelError> {
    let mut name_params = vec![];
    for name in order_attributes_names_not_in_db {
        let mut single_params = IndexMap::new();
        single_params.insert("_value".to_string(), Parameter::from(name));
        name_params.push(single_params);
    }
    let inserted_attr_names_ids = tx
        .bulk_insert("AttributeName", name_params, "_value")
        .await?;
    Ok(inserted_attr_names_ids)
}

async fn get_db_attribute_names(
    tx: &mut Transaction,
    attribute_name: &HashSet<String>,
) -> Result<Vec<DbAttribute>, gel_io_recorder::Error> {
    let mut attribute_name = attribute_name.into_iter().collect::<Vec<_>>();
    attribute_name.sort();
    let existing_attribute_names: Vec<DbAttribute> = tx
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
            IndexMap::from([(
                "attribute_names_json".to_string(),
                Parameter::from(
                    serde_json::to_value(&attribute_name).expect("names are serializable"),
                ),
            )]),
        )
        .await?;
    Ok(existing_attribute_names)
}
