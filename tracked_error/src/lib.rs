use std::panic::Location;

pub fn error_chain_to_pretty_formatted<E>(error: E) -> String
where
    E: std::error::Error,
{
    let mut error = &error as &dyn std::error::Error;
    let mut err = format!("{}", error);
    while let Some(inner_err) = error.source() {
        err.push_str(&format!("\nCaused by: \n{}", inner_err));
        error = inner_err;
    }
    err
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("Error at {location}")]
pub struct TrackedError<T: std::error::Error> {
    pub location: &'static Location<'static>,
    #[source]
    pub source: T,
}

impl<T: std::error::Error> From<T> for TrackedError<T> {
    #[track_caller]
    fn from(value: T) -> Self {
        Self {
            location: Location::caller(),
            source: value,
        }
    }
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


