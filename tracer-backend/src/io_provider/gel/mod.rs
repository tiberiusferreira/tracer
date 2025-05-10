use crate::io_provider::execution_recorder::database::Parameter;
use gel_protocol::value::Value;
use gel_protocol::value_opt::ValueOpt;
use std::collections::HashMap;
use std::fmt::Debug;

pub fn generate_insert_query(table: &str, columns: &HashMap<String, Parameter>) -> String {
    let mut column_set_queries = vec![];
    for (name, value) in columns {
        let bind_type = match value {
            Parameter::String(_) => "<str>".to_string(),
            Parameter::I32(_) => "<int32>".to_string(),
            Parameter::Uuid { cast_to_table, .. } => match cast_to_table {
                None => "<uuid>".to_string(),
                Some(cast_to_table) => {
                    format!("<{cast_to_table}><uuid>")
                }
            },
            Parameter::Json(_json) => "<json>".to_string(),
        };
        column_set_queries.push(format!("{name} := {bind_type}${name}"));
    }
    let column_set_query = column_set_queries.join(",\n");
    let query_str = format!(
        r#"insert {table}{{
    {column_set_query}
}}"#
    );
    query_str
}

pub fn params_to_gel(parameters: HashMap<String, Parameter>) -> HashMap<String, ValueOpt> {
    let mut hashmap = HashMap::new();
    for (k, v) in parameters {
        let a = match v {
            Parameter::String(v) => ValueOpt::from(Value::Str(v)),
            Parameter::I32(v) => ValueOpt::from(Value::Int32(v)),
            Parameter::Uuid { val, .. } => ValueOpt::from(val),
            Parameter::Json(json) => ValueOpt::from(Value::Json(
                gel_protocol::model::Json::new_unchecked(serde_json::to_string(&json).unwrap()),
            )),
        };
        hashmap.insert(k, a);
    }
    hashmap
}

// impl DatabaseAccessor {
//     pub async fn ro_query_one<T: Serialize + DeserializeOwned>(
//         &self,
//         query: &str,
//         parameters: HashMap<String, Parameter>,
//     ) -> Result<T, DatabaseError> {
//         query_one_recording(&self.client, self.execution_id, query, parameters).await
//     }
//     pub async fn insert(
//         &self,
//         table: &str,
//         columns: HashMap<String, Parameter>,
//     ) -> Result<Uuid, DatabaseError> {
//         let query = generate_insert_query(table, &columns);
//         let entity_id: Uuid =
//             query_one_recording(&self.client, self.execution_id, &query, columns.clone()).await?;
//         let old: Option<serde_json::Value> = None;
//         let mut new = serde_json::map::Map::new();
//         for (k, v) in &columns {
//             match v {
//                 Parameter::Uuid { val, .. } => {
//                     new.insert(k.to_string(), serde_json::Value::String(val.to_string()));
//                 }
//                 Parameter::String(val) => {
//                     new.insert(k.to_string(), serde_json::Value::String(val.to_string()));
//                 }
//                 Parameter::I32(val) => {
//                     new.insert(
//                         k.to_string(),
//                         serde_json::Value::Number(serde_json::Number::from(*val)),
//                     );
//                 }
//             }
//         }
//         let new = serde_json::Value::Object(new);
//         let new = Value::Json(gel_protocol::model::Json::new_unchecked(
//             serde_json::to_string_pretty(&new).unwrap(),
//         ));
//         let args = named_args! {
//               "entity_name" =>   table,
//               "entity_id" =>   entity_id,
//               "execution_id" =>   self.execution_id,
//               "new" =>   new,
//         };
//         self.client
//             .execute(
//                 "insert EntityChange{
//     entity_name := <str>$entity_name,
//     entity_id := <uuid>$entity_id,
//     execution := <Execution><uuid>$execution_id,
//     new := <json>$new
// };",
//                 &args,
//             )
//             .await
//             .unwrap();
//         Ok(entity_id)
//     }
// }
