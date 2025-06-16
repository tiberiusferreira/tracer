use crate::Parameter;
use gel_protocol::value::Value;
use gel_protocol::value_opt::ValueOpt;
use std::collections::HashMap;
use uuid::Uuid;

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
            Parameter::Datetime(_) => "<datetime>".to_string(),
            Parameter::Bool(_) => "<bool>".to_string(),
            Parameter::Date(_) => "<cal::local_date>".to_string(),
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

pub fn generate_update_query(
    table: &str,
    id: Uuid,
    columns: &HashMap<String, Parameter>,
) -> String {
    let mut column_update_queries = vec![];
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
            Parameter::Datetime(_) => "<datetime>".to_string(),
            Parameter::Bool(_) => "<bool>".to_string(),
            Parameter::Date(_) => "<cal::local_date>".to_string(),
        };
        column_update_queries.push(format!("{name} := {bind_type}${name}"));
    }
    let column_set_query = column_update_queries.join(",\n");
    let query_str = format!(
        "update {table}
        filter .id=<uuid>\"{}\"
        set {{
            {column_set_query}
        }};",
        id.to_string()
    );
    query_str
}

pub fn generate_select_query(table: &str, id: Uuid, columns: &[String]) -> String {
    assert!(!columns.is_empty());
    let columns = columns.join(", ");
    format!(
        "select {table} {{ {columns} }} filter .id=<uuid>\"{}\";",
        id.to_string()
    )
}

pub fn params_to_gel<'a>(parameters: HashMap<String, Parameter>) -> HashMap<String, ValueOpt> {
    let mut hashmap = HashMap::new();
    for (k, v) in parameters {
        let a = match v {
            Parameter::String(v) => ValueOpt::from(v),
            Parameter::I32(v) => ValueOpt::from(v),
            Parameter::Uuid { val, .. } => ValueOpt::from(val),
            Parameter::Json(json) => ValueOpt::from(Value::Json(
                gel_protocol::model::Json::new_unchecked(serde_json::to_string(&json).unwrap()),
            )),
            Parameter::Datetime(val) => {
                let val = val.map(|val| gel_protocol::model::Datetime::try_from(val).unwrap());
                ValueOpt::from(val)
            }
            Parameter::Bool(val) => ValueOpt::from(val),
            Parameter::Date(val) => {
                let val = val.map(|val| gel_protocol::model::LocalDate::try_from(val).unwrap());
                ValueOpt::from(val)
            }
        };
        hashmap.insert(k, a);
    }
    hashmap
}
