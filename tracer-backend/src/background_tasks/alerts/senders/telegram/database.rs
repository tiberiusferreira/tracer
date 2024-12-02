use crate::background_tasks::alerts::senders::telegram::TelegramConfig;
use sqlx::PgPool;
use tracing::instrument;
use tracked_error::SqlxError;

#[instrument(skip_all)]
pub async fn load_telegram_configs(con: &PgPool) -> Result<Vec<TelegramConfig>, SqlxError> {
    let res = sqlx::query_as!(
        TelegramConfig,
        "select id, api_key, chat_id from telegram_alert_config;"
    )
    .fetch_all(con)
    .await?;
    Ok(res)
}
