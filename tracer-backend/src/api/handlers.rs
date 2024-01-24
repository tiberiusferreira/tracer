use crate::api::ApiError;

use reqwest::StatusCode;

pub mod instance;
pub mod ui;
#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "severity_level", rename_all = "lowercase")]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
impl sqlx::postgres::PgHasArrayType for Severity {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("_severity_level")
    }
}

impl Severity {
    pub fn to_api(&self) -> api_structs::Severity {
        match self {
            Severity::Trace => api_structs::Severity::Trace,
            Severity::Debug => api_structs::Severity::Debug,
            Severity::Info => api_structs::Severity::Info,
            Severity::Warn => api_structs::Severity::Warn,
            Severity::Error => api_structs::Severity::Error,
        }
    }
}
impl From<api_structs::Severity> for Severity {
    fn from(value: api_structs::Severity) -> Self {
        match value {
            api_structs::Severity::Trace => Self::Trace,
            api_structs::Severity::Debug => Self::Debug,
            api_structs::Severity::Info => Self::Info,
            api_structs::Severity::Warn => Self::Warn,
            api_structs::Severity::Error => Self::Error,
        }
    }
}
impl TryFrom<&str> for Severity {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, ()> {
        match value.to_lowercase().as_str() {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err(()),
        }
    }
}

pub fn nanos_to_db_i64(nanos: u64) -> Result<i64, ApiError> {
    i64::try_from(nanos).map_err(|_| ApiError {
        code: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("Error converting nanos {nanos} to i64"),
    })
}
pub fn db_i64_to_nanos(db_i64: i64) -> Result<u64, ApiError> {
    u64::try_from(db_i64).map_err(|_| ApiError {
        code: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("Error converting db_i64 {db_i64} to u64"),
    })
}
