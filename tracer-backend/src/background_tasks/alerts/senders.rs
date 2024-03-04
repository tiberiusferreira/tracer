use backtraced_error::SqlxError;
use thiserror::Error;

pub mod slack;
pub mod telegram;

#[derive(Debug, Error)]
pub enum CriticalAlertSendError {
    #[error("CriticalAlertSendError")]
    Db(#[from] SqlxError),
}
