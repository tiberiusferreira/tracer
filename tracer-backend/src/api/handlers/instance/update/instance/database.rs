use api_structs::{InstanceGlobalId, InstanceUpdateId};
use sqlx::{Postgres, Transaction};
use tracing::instrument;
use tracked_error::SqlxError;

pub type InstanceDbId = i32;

#[instrument(skip_all)]
pub async fn get_last_instance_update_id(
    tx: &mut Transaction<'static, Postgres>,
    instance_db_id: i32,
) -> Result<Option<i32>, SqlxError> {
    Ok(sqlx::query_scalar!(
        "select max(update_id) from instance_update where instance_id=$1 limit 1",
        instance_db_id
    )
    .fetch_one(&mut **tx)
    .await?)
}

#[instrument(skip_all)]
pub async fn insert_instance_update(
    tx: &mut Transaction<'static, Postgres>,
    instance_db_id: i32,
    instance_update_id: InstanceUpdateId,
    export_buffer_size_bytes: u64,
) -> Result<(), SqlxError> {
    let export_buffer_size_bytes = export_buffer_size_bytes as i64;
    let instance_update_id = instance_update_id as i32;
    sqlx::query_scalar!(
        "insert into instance_update (instance_id, update_id, export_buffer_size_bytes) values ($1, $2, $3);",
        instance_db_id,
        instance_update_id,
        export_buffer_size_bytes
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn insert_instance_latest_log_filter_and_cpu_profile(
    tx: &mut Transaction<'static, Postgres>,
    instance_db_id: i32,
    log_filter: &str,
    cpu_profile: Option<Vec<u8>>,
) -> Result<(), SqlxError> {
    let cpu_profile = cpu_profile.as_ref().map(|e| e.as_slice());
    let _res = sqlx::query!(
        "insert into instance_latest_log_filter_and_cpu_profile (instance_id, log_filter, cpu_profile)
values ($1, $2, $3) on conflict (instance_id) do update
set log_filter=$2, cpu_profile=coalesce($3, instance_latest_log_filter_and_cpu_profile.cpu_profile);",
        instance_db_id,
        log_filter,
        cpu_profile
    )
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn get_instance_db_id(
    tx: &mut Transaction<'static, Postgres>,
    instance_global_id: InstanceGlobalId,
) -> Result<Option<InstanceDbId>, SqlxError> {
    Ok(sqlx::query_scalar!(
        "select id from instance where global_id = $1",
        instance_global_id
    )
    .fetch_optional(&mut **tx)
    .await?)
}
