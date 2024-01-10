use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

pub mod exporter;
pub mod time_conversion;
pub mod ui;

pub type TraceName = String;

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
// snake case because the DB expects lower case, so to_string() returns lowercase
// but we also use to_string() when sending this in query parameters
#[serde(rename_all = "snake_case")]
pub enum Env {
    Local,
    Dev,
    Stage,
    Prod,
}

impl TryFrom<&str> for Env {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_ascii_lowercase().as_str() {
            "local" => Ok(Env::Local),
            "dev" => Ok(Env::Dev),
            "stage" => Ok(Env::Stage),
            "prod" => Ok(Env::Prod),
            x => Err(format!("Invalid Env value: {}", x)),
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
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
