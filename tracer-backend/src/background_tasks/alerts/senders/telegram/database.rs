use crate::background_tasks::alerts::senders::telegram::TelegramConfig;
use crate::DB_INTERNAL_ERROR_CHAR_LIMIT;
use backtraced_error::SqlxError;
use chrono::NaiveDateTime;
use sqlx::PgPool;
use tracing::{debug, instrument};

#[instrument(skip_all)]
pub async fn load_telegram_configs(con: &PgPool) -> Result<Vec<TelegramConfig>, SqlxError> {
    #[derive(Debug, Clone)]
    struct RawTelegramConfig {
        id: i32,
        api_key: String,
        chat_id: String,
        min_alert_period_seconds: i64,
        last_alert_send_attempt: Option<NaiveDateTime>,
    }
    let res = sqlx::query_as!(
        RawTelegramConfig,
        "select telegram_alert_config.id,
       telegram_alert_config.api_key,
       telegram_alert_config.chat_id,
       telegram_alert_config.min_alert_period_seconds,
       Alert.last_alert_send_attempt
from telegram_alert_config
         left join (select telegram_alert.telegram_alert_config,
                           max(telegram_alert.created_at) as last_alert_send_attempt
                    from telegram_alert
                    group by telegram_alert_config) Alert
                   on Alert.telegram_alert_config = telegram_alert_config.id"
    )
    .fetch_all(con)
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, "getting TelegramConfig"))?
    .into_iter()
    .map(|e| TelegramConfig {
        id: e.id,
        api_key: e.api_key,
        min_alert_period_seconds: u64::try_from(e.min_alert_period_seconds).unwrap(),
        last_alert_send_attempt: e.last_alert_send_attempt,
        chat_id: e.chat_id,
    })
    .collect::<Vec<TelegramConfig>>();
    Ok(res)
}

#[instrument(skip_all)]
pub async fn insert_notification_in_db(
    con: &PgPool,
    telegram_config_id: i32,
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
        "insert into telegram_alert (telegram_alert_config, notification, send_error) \
    values ($1, $2, $3)",
        telegram_config_id,
        notification,
        send_error
    )
    .execute(con)
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, "inserting telegram_alert"))?;
    Ok(())
}
