use backtraced_error::{error_chain_to_pretty_formatted, SqlxError};
use sqlx::postgres::PgQueryResult;
use sqlx::PgPool;
use tracing::{error, info, info_span, instrument, Instrument};

#[instrument(skip_all)]
pub async fn delete_old_slack_notifications_logging_error(con: &PgPool) {
    let res: PgQueryResult = match sqlx::query!(
        "delete from slack_alert where created_at < (now() - INTERVAL '3 DAY');"
    )
    .execute(con)
    .instrument(info_span!("deleting_old_slack_notifications"))
    .await
    {
        Ok(res) => res,
        Err(err) => {
            let err_str = error_chain_to_pretty_formatted(SqlxError::from_sqlx_error(
                err,
                "deleting old slack notifications",
            ));
            error!("{err_str}");
            return;
        }
    };
    info!("Deleted {} records", res.rows_affected());
}
