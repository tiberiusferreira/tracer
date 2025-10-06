use crate::parameters::Parameter;
use gel_protocol::value::Value;
use gel_protocol::value_opt::ValueOpt;
use std::collections::HashMap;
use indexmap::IndexMap;


pub fn params_to_gel(parameters: IndexMap<String, Parameter>) -> HashMap<String, ValueOpt> {
    let mut hashmap = HashMap::new();
    for (k, v) in parameters {
        let a = match v {
            Parameter::String(v) => ValueOpt::from(v),
            Parameter::I32(v) => ValueOpt::from(v),
            Parameter::I64(v) => ValueOpt::from(v),
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
