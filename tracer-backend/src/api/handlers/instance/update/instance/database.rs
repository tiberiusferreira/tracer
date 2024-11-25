use api_structs::InstanceGlobalId;
use sqlx::{Postgres, Transaction};
use tracked_error::SqlxError;

pub type InstanceUpdateId = i32;
pub async fn insert_instance_update(
    tx: &mut Transaction<'static, Postgres>,
    instance_db_id: i32,
) -> Result<InstanceUpdateId, SqlxError> {
    Ok(sqlx::query_scalar!(
        "insert into instance_update (instance_id) values ($1) returning id",
        instance_db_id
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn get_instance_db_id(
    tx: &mut Transaction<'static, Postgres>,
    instance_global_id: InstanceGlobalId,
) -> Result<Option<InstanceUpdateId>, SqlxError> {
    Ok(sqlx::query_scalar!(
        "select id from instance where global_id = $1",
        instance_global_id
    )
    .fetch_optional(&mut **tx)
    .await?)
}
