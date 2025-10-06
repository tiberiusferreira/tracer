use reqwest::StatusCode;
use thiserror::Error;
use tracked_error::{ReqwestError, SerdeJsonError};

pub mod instance_registration;
pub mod instance_update_sender;
pub mod request_compression;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Unexpected status: {status} Body {body}")]
    NonOkResponse { status: StatusCode, body: String },
    #[error("Unexpected response body. Status: {status}")]
    UnexpectedResponseBody {
        #[source]
        error: SerdeJsonError,
        status: StatusCode,
    },
    #[error("Http error")]
    Http(#[from] ReqwestError),
}
