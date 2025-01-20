use thiserror::Error;
use tracked_error::{EdgeDBError, SerdeJsonError};

#[derive(Debug, Error)]
pub enum EdgeDBOrSerdeJson {
    #[error("EdgeDBError")]
    EdgeDB(#[from] EdgeDBError),
    #[error("SerdeJson")]
    SerdeJson(#[from] SerdeJsonError),
}
