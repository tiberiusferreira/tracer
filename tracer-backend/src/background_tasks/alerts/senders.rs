use thiserror::Error;
use tracked_error::SqlxError;

pub mod slack;
pub mod telegram;

#[derive(Debug, Error)]
pub enum CriticalAlertSendError {
    #[error("CriticalAlertSendError")]
    Db(#[from] SqlxError),
}
