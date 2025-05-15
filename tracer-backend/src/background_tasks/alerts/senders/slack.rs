use chrono::NaiveDateTime;
use reqwest::Response;
use reqwest::header::InvalidHeaderValue;
use std::fmt::Formatter;
use thiserror::Error;
use tracked_error::{ReqwestError, error_chain_to_pretty_formatted};

pub mod database;

#[expect(unused)]
pub async fn send_to_slack_and_update_database(
    // _con: &PgPool,
    _notification: &str,
) -> Result<(), ()> {
    // let slack_configs = database::load_slack_configs(&con).await?;
    // info!("Slack Configs {:?}", slack_configs);
    // for s in slack_configs {
    //     info!("Processing {:?}", s);
    //     if let Some(last_alert_send_attempt) = s.last_alert_send_attempt {
    //         info!("Last alert send attempt: {}", last_alert_send_attempt);
    //         let duration_since_last_attempt =
    //             chrono::Utc::now().naive_utc() - last_alert_send_attempt;
    //         let duration_since_last_attempt =
    //             u64::try_from(duration_since_last_attempt.num_seconds()).unwrap_or(0);
    //         info!(
    //             "Last notification sent {} seconds ago",
    //             duration_since_last_attempt
    //         );
    //         if s.min_alert_period_seconds < duration_since_last_attempt {
    //             info!("Clear to send new notifications");
    //         } else {
    //             info!("Too soon to send notifications, skipping it now");
    //             continue;
    //         }
    //     } else {
    //         info!("Sending first notification ever!");
    //     }
    //     let error_str =
    //         send_slack_msg_logging_error(&s.bot_user_oauth_token, &s.channel_id, &notification)
    //             .await
    //             .err();
    //     database::insert_notification_in_db(con, s.id, &notification, error_str).await?;
    // }
    // Ok(())
    unimplemented!()
}

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
        .map_err(ReqwestError::from)?;
    let text_req = client
        .post("https://slack.com/api/chat.postMessage")
        .query(&[
            ("channel", channel_id.to_string()),
            ("text", notification.to_string()),
        ])
        .build()
        .map_err(ReqwestError::from)?;
    let text_send_resp = client.execute(text_req).await.map_err(ReqwestError::from)?;
    check_response(text_send_resp)
        .await
        .map_err(|e| SlackResponseError {
            context: format!("Sending slack msg {}", notification),
            error: e,
        })?;
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct SlackResponse {
    #[allow(unused)]
    ok: bool,
    #[allow(dead_code)] // used only to print
    #[serde(default)]
    error: Option<String>,
}

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

#[allow(unused)]
async fn send_slack_msg_logging_error(
    bot_token: &str,
    channel_id: &str,
    notification: &str,
) -> Result<(), String> {
    if let Err(e) = send_slack_msg(bot_token, channel_id, &notification).await {
        let error_str = error_chain_to_pretty_formatted(e);
        Err(error_str)
    } else {
        Ok(())
    }
}

#[derive(Clone)]
pub struct SlackConfig {
    #[allow(unused)]
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
#[error("Unexpected Slack Response. Context: {context}\n{error}")]
pub struct SlackResponseError {
    pub context: String,
    pub error: String,
}

#[derive(Debug, Error)]
#[error("InvalidHeaderError Context: {context}\n")]
pub struct InvalidHeaderError {
    #[source]
    pub source: InvalidHeaderValue,
    pub context: String,
}

impl InvalidHeaderError {
    #[allow(unused)]
    pub fn from_invalid_header_error<S: Into<String>>(
        e: InvalidHeaderValue,
        context: S,
    ) -> InvalidHeaderError {
        Self {
            source: e,
            context: context.into(),
        }
    }
}
