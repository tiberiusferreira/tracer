use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Parameter {
    String(Option<String>),
    Date(Option<NaiveDate>),
    Datetime(Option<DateTime<Utc>>),
    Bool(Option<bool>),
    Json(Option<serde_json::Value>),
    I32(Option<i32>),
    I32Array(Option<Vec<i32>>),
    I64(Option<i64>),
}

impl Parameter {
    pub fn as_json(&self) -> serde_json::Value {
        let err = "parameter serialization should never fail";
        match self {
            Parameter::String(val) => serde_json::to_value(val).expect(err),
            Parameter::I32(val) => serde_json::to_value(val).expect(err),
            Parameter::I64(val) => serde_json::to_value(val).expect(err),
            Parameter::Json(val) => serde_json::to_value(val).expect(err),
            Parameter::Datetime(val) => serde_json::to_value(val).expect(err),
            Parameter::Bool(val) => serde_json::to_value(val).expect(err),
            Parameter::Date(val) => serde_json::to_value(val).expect(err),
            Parameter::I32Array(val) => serde_json::to_value(val).expect(err)
        }
    }
}

impl From<bool> for Parameter {
    fn from(value: bool) -> Self {
        Parameter::Bool(Some(value))
    }
}

impl From<Option<bool>> for Parameter {
    fn from(value: Option<bool>) -> Self {
        Parameter::Bool(value)
    }
}

impl From<NaiveDate> for Parameter {
    fn from(value: NaiveDate) -> Self {
        Parameter::Date(Some(value))
    }
}

impl From<Option<NaiveDate>> for Parameter {
    fn from(value: Option<NaiveDate>) -> Self {
        Parameter::Date(value)
    }
}

impl From<DateTime<Utc>> for Parameter {
    fn from(value: DateTime<Utc>) -> Self {
        Parameter::Datetime(Some(value))
    }
}

impl From<Option<DateTime<Utc>>> for Parameter {
    fn from(value: Option<DateTime<Utc>>) -> Self {
        Parameter::Datetime(value)
    }
}

impl From<String> for Parameter {
    fn from(value: String) -> Self {
        Parameter::String(Some(value))
    }
}

impl From<Option<String>> for Parameter {
    fn from(value: Option<String>) -> Self {
        Parameter::String(value)
    }
}

impl From<&String> for Parameter {
    fn from(value: &String) -> Self {
        Parameter::String(Some(value.clone()))
    }
}

impl From<Option<&String>> for Parameter {
    fn from(value: Option<&String>) -> Self {
        Parameter::String(value.map(String::to_string))
    }
}

impl From<&str> for Parameter {
    fn from(value: &str) -> Self {
        Parameter::String(Some(value.to_string()))
    }
}

impl From<Option<&str>> for Parameter {
    fn from(value: Option<&str>) -> Self {
        Parameter::String(value.map(String::from))
    }
}

impl From<serde_json::Value> for Parameter {
    fn from(value: serde_json::Value) -> Self {
        Parameter::Json(Some(value))
    }
}

impl From<Option<serde_json::Value>> for Parameter {
    fn from(value: Option<serde_json::Value>) -> Self {
        Parameter::Json(value)
    }
}

impl From<i32> for Parameter {
    fn from(value: i32) -> Self {
        Parameter::I32(Some(value))
    }
}

impl From<Option<i32>> for Parameter {
    fn from(value: Option<i32>) -> Self {
        Parameter::I32(value)
    }
}


impl From<i64> for Parameter {
    fn from(value: i64) -> Self {
        Parameter::I64(Some(value))
    }
}

impl From<Option<i64>> for Parameter {
    fn from(value: Option<i64>) -> Self {
        Parameter::I64(value)
    }
}