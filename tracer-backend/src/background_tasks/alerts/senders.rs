use backtraced_error::SqlxError;
use thiserror::Error;

pub mod slack;

#[derive(Debug, Error)]
pub enum CriticalAlertSendError {
    #[error("CriticalAlertSendError")]
    Db(#[from] SqlxError),
}
