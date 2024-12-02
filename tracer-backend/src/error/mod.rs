use thiserror::Error;
use tracked_error::{SerdeJsonError, SqlxError};

#[derive(Debug, Error)]
pub enum SqlxOrSerdeJson {
    #[error("SqlxError")]
    Sqlx(#[from] SqlxError),
    #[error("SerdeJson")]
    SerdeJson(#[from] SerdeJsonError),
}
