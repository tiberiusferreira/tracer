use deepsize::DeepSizeOf;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

pub mod instance;
pub mod time_conversion;
pub mod ui;

pub trait Endpoint {
    const PATH: &'static str;
    const METHOD: &'static str;
    type RequestBody: Serialize + DeserializeOwned;
    type ResponseBody: Serialize + DeserializeOwned;
}

pub type TraceName = String;
pub type SpanId = u64;
pub type InstanceGlobalId = uuid::Uuid;
pub type TraceId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct ServiceId {
    /// tracer-backend
    pub name: String,
    /// Local
    pub env: Env,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, Hash)]
// snake case because the DB expects lower case, so to_string() returns lowercase
// but we also use to_string() when sending this in query parameters
#[serde(rename_all = "snake_case")]
pub enum Env {
    Local,
    Dev,
    Stage,
    Prod,
    Other(String),
}

impl From<String> for Env {
    fn from(value: String) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "local" => Env::Local,
            "dev" => Env::Dev,
            "stage" => Env::Stage,
            "prod" => Env::Prod,
            x => Env::Other(x.to_ascii_lowercase()),
        }
    }
}
impl Display for Env {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Env::Local => f.write_str("local"),
            Env::Dev => f.write_str("dev"),
            Env::Stage => f.write_str("stage"),
            Env::Prod => f.write_str("prod"),
            Env::Other(x) => f.write_str(x.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize, DeepSizeOf)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Display for Severity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Trace => f.write_str("trace"),
            Severity::Debug => f.write_str("debug"),
            Severity::Info => f.write_str("info"),
            Severity::Warn => f.write_str("warn"),
            Severity::Error => f.write_str("error"),
        }
    }
}

impl FromStr for Severity {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err(()),
        }
    }
}
