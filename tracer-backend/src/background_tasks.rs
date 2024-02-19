use crate::DB_INTERNAL_ERROR_CHAR_LIMIT;
use backtraced_error::SqlxError;
use sqlx::PgPool;
use tracing::{debug, instrument};

pub mod alerts;

#[instrument(skip_all)]
async fn insert_notification_in_db(
    con: &PgPool,
    bot_token: &str,
    channel_id: &str,
    notification: &str,
    send_error: Option<String>,
) -> Result<(), SqlxError> {
    debug!("updating update_last_notification_time");
    let send_error = send_error.map(|e| {
        if e.chars().count() > DB_INTERNAL_ERROR_CHAR_LIMIT {
            e.chars()
                .into_iter()
                .take(DB_INTERNAL_ERROR_CHAR_LIMIT)
                .collect::<String>()
        } else {
            e
        }
    });
    sqlx::query!(
        "insert into slack_alert (bot_token, channel_id, notification, send_error) \
    values ($1, $2, $3, $4)",
        bot_token,
        channel_id,
        notification,
        send_error
    )
    .execute(con)
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, "inserting slack_alert"))?;
    Ok(())
}
