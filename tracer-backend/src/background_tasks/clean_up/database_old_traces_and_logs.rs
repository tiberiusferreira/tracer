use backtraced_error::{error_chain_to_pretty_formatted, SqlxError};
use sqlx::postgres::PgQueryResult;
use sqlx::PgPool;
use tracing::{error, info, info_span, instrument, Instrument};

#[instrument(skip_all)]
pub async fn delete_old_traces_logging_error(con: &PgPool) {
    let res: PgQueryResult =
        match sqlx::query!("delete from trace where timestamp < (EXTRACT(epoch FROM now() - INTERVAL '1 DAY') * 1000000000);")
            .execute(con)
            .instrument(info_span!("deleting_old_traces"))
            .await {
            Ok(res) => { res }
            Err(err) => {
                let err_str = error_chain_to_pretty_formatted(SqlxError::from_sqlx_error(err, "deleting old traces"));
                error!("{err_str}");
                return;
            }
        };
    info!("Deleted {} records", res.rows_affected());
}

#[instrument(skip_all)]
pub async fn delete_old_orphan_events_logging_error(con: &PgPool) {
    let res: PgQueryResult =
        match sqlx::query!("delete from orphan_event where timestamp < (EXTRACT(epoch FROM now() - INTERVAL '1 DAY') * 1000000000);")
            .execute(con)
            .instrument(info_span!("deleting_old_orphan_events"))
            .await {
            Ok(res) => { res }
            Err(err) => {
                let err_str = error_chain_to_pretty_formatted(SqlxError::from_sqlx_error(err, "deleting old orphan events"));
                error!("{err_str}");
                return;
            }
        };
    info!("Deleted {} records", res.rows_affected());
}
