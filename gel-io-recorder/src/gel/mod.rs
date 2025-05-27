use crate::Parameter;
use gel_protocol::value::Value;
use gel_protocol::value_opt::ValueOpt;
use std::collections::HashMap;

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
            Parameter::NoneString => "<str>".to_string(),
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
            Parameter::Datetime(val) => ValueOpt::from(Value::Datetime(
                gel_protocol::model::Datetime::try_from(val).unwrap(),
            )),
            Parameter::Bool(val) => ValueOpt::from(Value::Bool(val)),
            Parameter::Date(val) => ValueOpt::from(Value::LocalDate(
                gel_protocol::model::LocalDate::try_from(val).unwrap(),
            )),
            Parameter::NoneString => ValueOpt::from(Option::<String>::None),
        };
        hashmap.insert(k, a);
    }
    hashmap
}
