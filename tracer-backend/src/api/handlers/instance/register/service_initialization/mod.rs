use api_structs::ServiceId;
use sqlx::{Postgres, Transaction};
use tracing::instrument;
use tracked_error::SqlxError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database error")]
    Database(#[from] SqlxError),
}

impl From<sqlx::Error> for Error {
    #[track_caller]
    fn from(value: sqlx::Error) -> Self {
        Self::Database(SqlxError::from(value))
    }
}

pub type ServiceDbId = i32;

#[instrument(skip_all)]
pub async fn get_service_db_id(
    con: &mut Transaction<'static, Postgres>,
    service_id: &ServiceId,
) -> Result<Option<ServiceDbId>, SqlxError> {
    let service_id = sqlx::query_scalar!(
        "select id from service where env=$1 and name=$2 for share",
        service_id.env.to_string(),
        service_id.name
    )
    .fetch_optional(&mut **con)
    .await?;
    Ok(service_id)
}

#[instrument(skip_all)]
pub async fn insert_service(
    con: &mut Transaction<'static, Postgres>,
    service_id: &ServiceId,
    log_filter: String,
) -> Result<ServiceDbId, SqlxError> {
    let service_db_id = sqlx::query_scalar!(
        "insert into service (env, name) values ($1, $2) returning id",
        service_id.env.to_string(),
        service_id.name
    )
    .fetch_one(&mut **con)
    .await?;
    sqlx::query!(
        "insert into service_rust_log_setting (service_id, rust_log) values ($1, $2)",
        service_db_id,
        log_filter
    )
    .execute(&mut **con)
    .await?;
    Ok(service_db_id)
}
