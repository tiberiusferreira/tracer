use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
#[derive(Debug, Clone, Serialize, Deserialize, Error)]
pub enum Error {
    #[error("Internal {0}")]
    Internal(String),
    #[error("Serde {0}")]
    Serde(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub query_text: String,
    pub parameters: HashMap<String, Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Parameter {
    Uuid {
        val: uuid::Uuid,
        cast_to_table: Option<String>,
    },
    String(String),
    Json(serde_json::Value),
    I32(i32),
}
