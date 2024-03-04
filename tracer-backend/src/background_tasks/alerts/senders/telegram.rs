use crate::background_tasks::alerts::AlertingError;
use backtraced_error::error_chain_to_pretty_formatted;
use chrono::NaiveDateTime;
use frankenstein::{Error, SendMessageParams, TelegramApi};
use sqlx::PgPool;
use std::fmt::Formatter;
use tracing::{debug, error, info, instrument};
mod database;
#[instrument(skip_all)]
pub async fn send_to_telegram_and_update_database(
    con: &PgPool,
    notification: &str,
) -> Result<(), AlertingError> {
    let telegram_configs = database::load_telegram_configs(&con).await?;
    info!("telegram_configs={:?}", telegram_configs);
    for s in telegram_configs {
        info!("Processing {:?}", s);
        if let Some(last_alert_send_attempt) = s.last_alert_send_attempt {
            info!("Last alert send attempt: {}", last_alert_send_attempt);
            let duration_since_last_attempt =
                chrono::Utc::now().naive_utc() - last_alert_send_attempt;
            let duration_since_last_attempt =
                u64::try_from(duration_since_last_attempt.num_seconds()).unwrap_or(0);
            info!(
                "Last notification sent {} seconds ago",
                duration_since_last_attempt
            );
            if s.min_alert_period_seconds < duration_since_last_attempt {
                info!("Clear to send new notifications");
            } else {
                info!("Too soon to send notifications, skipping it now");
                continue;
            }
        } else {
            info!("Sending first notification ever!");
        }
        let error_str = send_telegram_msg_logging_error(&s.api_key, &s.chat_id, &notification)
            .await
            .err();
        database::insert_notification_in_db(con, s.id, &notification, error_str).await?;
    }
    Ok(())
}

async fn send_telegram_msg_logging_error(
    api_key: &str,
    chat_id: &str,
    notification: &str,
) -> Result<(), String> {
    return if let Err(e) = send_telegram_msg(api_key, chat_id, &notification).await {
        let error_str = error_chain_to_pretty_formatted(e);
        error!("{error_str}");
        Err(error_str)
    } else {
        Ok(())
    };
}

async fn send_telegram_msg(api_key: &str, chat_id: &str, notification: &str) -> Result<(), Error> {
    let api = frankenstein::Api::new(api_key);
    let resp = api.send_message(
        &SendMessageParams::builder()
            .chat_id(chat_id.to_string())
            .text(notification.to_string())
            .build(),
    )?;
    debug!("{resp:#?}");
    Ok(())
}

#[derive(Clone)]
pub struct TelegramConfig {
    pub id: i32,
    pub api_key: String,
    pub chat_id: String,
    pub min_alert_period_seconds: u64,
    pub last_alert_send_attempt: Option<NaiveDateTime>,
}

impl std::fmt::Debug for TelegramConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramConfig")
            .field("id", &self.id)
            .field("api_key", &self.api_key.chars().take(5).collect::<String>())
            .field("chat_id", &self.chat_id.chars().take(5).collect::<String>())
            .field("min_alert_period_seconds", &self.min_alert_period_seconds)
            .field("last_alert_send_attempt", &self.last_alert_send_attempt)
            .finish()
    }
}
