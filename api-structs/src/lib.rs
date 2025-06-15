use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub mod execution;
pub mod instance;
pub mod time_conversion;
pub mod ui;

pub trait Endpoint {
    const PATH: &'static str;
    const METHOD: &'static str;
    type RequestBody: Serialize + DeserializeOwned;
    type QueryParameters: Serialize + DeserializeOwned;
    type ResponseBody: Serialize + DeserializeOwned;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct ServiceId {
    /// tracer-backend
    pub name: String,
    /// Local
    pub env: String,
}
