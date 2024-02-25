use crate::background_tasks::alerts::senders::slack::SlackConfig;
use crate::DB_INTERNAL_ERROR_CHAR_LIMIT;
use backtraced_error::SqlxError;
use chrono::NaiveDateTime;
use sqlx::PgPool;
use tracing::{debug, instrument};

#[instrument(skip_all)]
pub async fn load_slack_configs(con: &PgPool) -> Result<Vec<SlackConfig>, SqlxError> {
    #[derive(Debug, Clone)]
    struct RawSlackConfig {
        id: i32,
        bot_user_oauth_token: String,
        channel_id: String,
        min_alert_period_seconds: i64,
        last_alert_send_attempt: Option<NaiveDateTime>,
    }
    let res = sqlx::query_as!(
        RawSlackConfig,
        "select slack_alert_config.id,
        slack_alert_config.bot_user_oauth_token,
       slack_alert_config.channel_id,
       slack_alert_config.min_alert_period_seconds,
       Alert.last_alert_send_attempt
from slack_alert_config
         left join (select slack_alert.slack_alert_config_id,
                           max(slack_alert.created_at) as last_alert_send_attempt
                    from slack_alert
                    group by slack_alert_config_id) Alert
                   on Alert.slack_alert_config_id = slack_alert_config.id"
    )
    .fetch_all(con)
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, "getting SlackConfig"))?
    .into_iter()
    .map(|e| SlackConfig {
        id: e.id,
        bot_user_oauth_token: e.bot_user_oauth_token,
        channel_id: e.channel_id,
        min_alert_period_seconds: u64::try_from(e.min_alert_period_seconds).unwrap(),
        last_alert_send_attempt: e.last_alert_send_attempt,
    })
    .collect::<Vec<SlackConfig>>();
    Ok(res)
}

#[instrument(skip_all)]
pub async fn insert_notification_in_db(
    con: &PgPool,
    slack_config_id: i32,
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
        "insert into slack_alert (slack_alert_config_id, notification, send_error) \
    values ($1, $2, $3)",
        slack_config_id,
        notification,
        send_error
    )
    .execute(con)
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, "inserting slack_alert"))?;
    Ok(())
}
