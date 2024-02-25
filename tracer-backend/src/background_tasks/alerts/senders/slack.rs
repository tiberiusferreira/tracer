use crate::background_tasks::alerts::AlertingError;
use backtraced_error::{
    error_chain_to_pretty_formatted, OptionBacktracePrettyPrinter, ReqwestError,
};
use chrono::NaiveDateTime;
use reqwest::header::InvalidHeaderValue;
use reqwest::Response;
use sqlx::PgPool;
use std::fmt::Formatter;
use thiserror::Error;
use tracing::{error, info, instrument};

pub mod database;

#[instrument(skip_all)]
pub async fn send_to_slack_and_update_database(
    con: &PgPool,
    notification: &str,
) -> Result<(), AlertingError> {
    let slack_configs = database::load_slack_configs(&con).await?;
    info!("Slack Configs {:?}", slack_configs);
    for s in slack_configs {
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
        let error_str =
            send_slack_msg_logging_error(&s.bot_user_oauth_token, &s.channel_id, &notification)
                .await
                .err();
        database::insert_notification_in_db(con, s.id, &notification, error_str).await?;
    }
    Ok(())
}

#[instrument(skip_all)]
async fn send_slack_msg(
    bot_token: &str,
    channel_id: &str,
    notification: &str,
) -> Result<(), SlackSendError> {
    let mut default_header = reqwest::header::HeaderMap::new();
    default_header.insert(
        "Authorization",
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", bot_token))
            .map_err(|e| InvalidHeaderError::from_invalid_header_error(e, "creating header"))?,
    );
    let client = reqwest::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .default_headers(default_header)
        .build()
        .map_err(|e| ReqwestError::from_reqwest_error(e, "building reqwest client"))?;
    let text_req = client
        .post("https://slack.com/api/chat.postMessage")
        .query(&[
            ("channel", channel_id.to_string()),
            ("text", notification.to_string()),
        ])
        .build()
        .map_err(|e| ReqwestError::from_reqwest_error(e, "building post request"))?;
    let text_send_resp = client
        .execute(text_req)
        .await
        .map_err(|e| ReqwestError::from_reqwest_error(e, "sending request"))?;
    check_response(text_send_resp)
        .await
        .map_err(|e| SlackResponseError {
            context: format!("Sending slack msg {}", notification),
            error: e,
            backtrace: OptionBacktracePrettyPrinter::capture(),
        })?;
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct SlackResponse {
    ok: bool,
    #[allow(dead_code)] // used only to print
    #[serde(default)]
    error: Option<String>,
}

#[instrument(skip_all)]
async fn check_response(text_send_resp: Response) -> Result<(), String> {
    let status = text_send_resp.status();
    let body = text_send_resp.text().await.map_err(|e| e.to_string())?;
    let resp_as_json: SlackResponse =
        serde_json::from_str(&body).map_err(|e| format!("Unexpected response: {e:#?}.\n{body}"))?;
    return if !status.is_success() || !resp_as_json.ok {
        Err(format!("Got error in response: {:#?}", resp_as_json))
    } else {
        Ok(())
    };
}

async fn send_slack_msg_logging_error(
    bot_token: &str,
    channel_id: &str,
    notification: &str,
) -> Result<(), String> {
    return if let Err(e) = send_slack_msg(bot_token, channel_id, &notification).await {
        let error_str = error_chain_to_pretty_formatted(e);
        error!("{error_str}");
        Err(error_str)
    } else {
        Ok(())
    };
}

#[derive(Clone)]
pub struct SlackConfig {
    pub id: i32,
    pub bot_user_oauth_token: String,
    pub channel_id: String,
    pub min_alert_period_seconds: u64,
    pub last_alert_send_attempt: Option<NaiveDateTime>,
}

impl std::fmt::Debug for SlackConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlackConfig")
            .field(
                "bot_token",
                &self
                    .bot_user_oauth_token
                    .chars()
                    .take(5)
                    .collect::<String>(),
            )
            .field(
                "channel_id",
                &self.channel_id.chars().take(5).collect::<String>(),
            )
            .field("min_alert_period_seconds", &self.min_alert_period_seconds)
            .field("last_alert_send_attempt", &self.last_alert_send_attempt)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum SlackSendError {
    #[error(transparent)]
    Header(#[from] InvalidHeaderError),
    #[error(transparent)]
    Http(#[from] ReqwestError),
    #[error(transparent)]
    SlackError(#[from] SlackResponseError),
}

#[derive(Debug, Error)]
#[error("Unexpected Slack Response. Context: {context}\n{error}\n{backtrace}")]
pub struct SlackResponseError {
    pub context: String,
    pub error: String,
    pub backtrace: OptionBacktracePrettyPrinter,
}

#[derive(Debug, Error)]
#[error("InvalidHeaderError Context: {context}\n{backtrace}")]
pub struct InvalidHeaderError {
    #[source]
    pub source: InvalidHeaderValue,
    pub context: String,
    pub backtrace: OptionBacktracePrettyPrinter,
}

impl InvalidHeaderError {
    pub fn from_invalid_header_error<S: Into<String>>(
        e: InvalidHeaderValue,
        context: S,
    ) -> InvalidHeaderError {
        Self {
            source: e,
            context: context.into(),
            backtrace: OptionBacktracePrettyPrinter::capture(),
        }
    }
}
