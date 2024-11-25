use api_structs::InstanceGlobalId;
use sqlx::{Postgres, Transaction};
use tracing::instrument;
use tracked_error::SqlxError;

#[instrument(skip_all)]
pub async fn get_instance_service_log_filter(
    con: &mut Transaction<'static, Postgres>,
    instance_id: InstanceGlobalId,
) -> Result<Option<String>, SqlxError> {
    let log_filter = sqlx::query_scalar!("select service_rust_log_setting.rust_log
    from instance
         inner join service_rust_log_setting on service_rust_log_setting.service_id = instance.service_id
     where instance.global_id=$1;", instance_id)
        .fetch_optional(&mut **con)
        .await?;
    Ok(log_filter)
}
