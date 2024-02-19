use crate::background_tasks::alerts::senders::slack::SlackConfig;
use backtraced_error::SqlxError;
use chrono::NaiveDateTime;
use sqlx::PgPool;
use tracing::instrument;

#[instrument(skip_all)]
pub async fn load_slack_configs(con: &PgPool) -> Result<Vec<SlackConfig>, SqlxError> {
    #[derive(Debug, Clone)]
    struct RawSlackConfig {
        bot_token: String,
        channel_id: String,
        min_alert_period_seconds: i64,
        last_alert_send_attempt: Option<NaiveDateTime>,
    }
    let res = sqlx::query_as!(
        RawSlackConfig,
        "select slack_alert_config.bot_token,
       slack_alert_config.channel_id,
       slack_alert_config.min_alert_period_seconds,
       Alert.last_alert_send_attempt
from slack_alert_config
         left join (select bot_token,
                           channel_id,
                           max(slack_alert.created_at) as last_alert_send_attempt
                    from slack_alert
                    group by bot_token, channel_id) Alert
                   on Alert.channel_id = slack_alert_config.channel_id
                       and Alert.bot_token = slack_alert_config.bot_token;"
    )
    .fetch_all(con)
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, "getting SlackConfig"))?
    .into_iter()
    .map(|e| SlackConfig {
        bot_token: e.bot_token,
        channel_id: e.channel_id,
        min_alert_period_seconds: u64::try_from(e.min_alert_period_seconds).unwrap(),
        last_alert_send_attempt: e.last_alert_send_attempt,
    })
    .collect::<Vec<SlackConfig>>();
    Ok(res)
}
