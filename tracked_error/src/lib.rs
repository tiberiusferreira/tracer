use std::panic::Location;
use thiserror::__private::AsDynError;

pub fn error_chain_to_pretty_formatted<E>(error: E) -> String
where
    E: std::error::Error,
{
    let mut error = error.as_dyn_error();
    let mut err = format!("{}", error);
    while let Some(inner_err) = error.source() {
        err.push_str(&format!("\nCaused by: \n{}", inner_err));
        error = inner_err;
    }
    err
}

#[derive(Debug, thiserror::Error)]
#[error("SerdeJsonError\nSample:{bad_input_sample}\nat {location}")]
pub struct SerdeJsonError {
    #[source]
    pub source: serde_json::Error,
    pub bad_input_sample: String,
    pub location: &'static Location<'static>,
}

impl SerdeJsonError {
    #[track_caller]
    pub fn from_serde_json_error(source: serde_json::Error, bad_input_sample: String) -> Self {
        Self {
            source,
            bad_input_sample,
            location: Location::caller(),
        }
    }
}

#[cfg(feature = "reqwest")]
#[derive(Debug, thiserror::Error)]
#[error("ReqwestError\nat {location}")]
pub struct ReqwestError {
    #[source]
    pub source: reqwest::Error,
    pub location: &'static Location<'static>,
}

#[cfg(feature = "reqwest")]
impl From<reqwest::Error> for ReqwestError {
    #[track_caller]
    fn from(source: reqwest::Error) -> Self {
        Self {
            source,
            location: Location::caller(),
        }
    }
}

#[cfg(feature = "sqlx")]
#[derive(Debug, thiserror::Error)]
#[error("SqlxError\nat {location}")]
pub struct SqlxError {
    #[source]
    pub source: sqlx::Error,
    pub location: &'static Location<'static>,
}

#[cfg(feature = "sqlx")]
impl From<sqlx::error::Error> for SqlxError {
    #[track_caller]
    fn from(source: sqlx::Error) -> Self {
        Self {
            source,
            location: Location::caller(),
        }
    }
}

#[cfg(feature = "edgedb-tokio")]
#[derive(Debug, thiserror::Error)]
#[error("EdgeDBError at {location}")]
pub struct EdgeDBError {
    #[source]
    pub source: edgedb_tokio::Error,
    pub location: &'static Location<'static>,
}

#[cfg(feature = "edgedb-tokio")]
impl From<edgedb_tokio::Error> for EdgeDBError {
    #[track_caller]
    fn from(source: edgedb_tokio::Error) -> Self {
        Self {
            source,
            location: Location::caller(),
        }
    }
}
