use sqlx::PgPool;
use thiserror::Error;
use tracing::{error, instrument};
use tracked_error::SqlxError;
use valuable_derive::Valuable;
mod database;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error in TelegramLib")]
    TelegramLib(#[from] frankenstein::Error),
    #[error("Db Error")]
    DbError(#[from] SqlxError),
}

#[instrument(skip_all)]
pub async fn send_alerts(_con: PgPool, _max_alerts_to_send: u32) -> Result<(), Error> {
    //     let mut tx = con.begin().await.map_err(SqlxError::from)?;
    //     let last_unsent_alerts = sqlx::query_as!(UnsentAlerts,
    //     "select series_alert_check.id, series_alert_check.alert_message as \"alert_message!\", series_alert_check.created_at
    // from series_alert_check
    //          inner join series on series.id = series_alert_check.series_id
    // where alert_message is not null
    //   and notification_sent = false
    // order by created_at desc limit $1;", max_alerts_to_send as i64)
    //         .fetch_all(&mut *tx)
    //         .await.map_err(SqlxError::from)?;
    //     let telegram_configs = database::load_telegram_configs(&con).await?;
    //     info!(
    //         telegram_configs = telegram_configs.as_value(),
    //         "loaded telegram configs"
    //     );
    //     for telegram_config in telegram_configs {
    //         info!(
    //             telegram_config = telegram_config.as_value(),
    //             alert_count = last_unsent_alerts.len(),
    //             "sending alerts to using telegram config"
    //         );
    //         let client = frankenstein::AsyncApi::new(&telegram_config.api_key);
    //         for unsent_alert in &last_unsent_alerts {
    //             info!(?unsent_alert, "sending alert");
    //             match send_telegram_alert(&unsent_alert.alert_message, &client, &telegram_config).await
    //             {
    //                 Ok(_) => {
    //                     sqlx::query!(
    //                         "update series_alert_check set notification_sent=true where id=$1",
    //                         unsent_alert.id
    //                     )
    //                     .execute(&mut *tx)
    //                     .await
    //                     .map_err(SqlxError::from)?;
    //                 }
    //                 Err(e) => {
    //                     error!(
    //                         telegram_config = telegram_config.as_value(),
    //                         ?unsent_alert,
    //                         ?e,
    //                         "failed to send alert"
    //                     );
    //                 }
    //             }
    //         }
    //     }
    //     tx.commit().await.map_err(SqlxError::from)?;
    //     Ok(())
    unimplemented!()
}

// #[instrument(skip_all)]
// async fn send_telegram_alert(
//     notification: &str,
//     telegram_client: &AsyncApi,
//     telegram_config: &TelegramConfig,
// ) -> Result<(), Error> {
//     info!(
//         telegram_config = telegram_config.as_value(),
//         notification, "sending notification"
//     );
//     send_telegram_msg(&telegram_client, &telegram_config.chat_id, notification).await?;
//
//     Ok(())
// }

// #[allow(unused)]
// async fn send_telegram_msg(
//     _client: &AsyncApi,
//     _chat_id: &str,
//     _notification: &str,
// ) -> Result<(), frankenstein::Error> {
//     // let resp = client
//     //     .send_message(
//     //         &SendMessageParams::builder()
//     //             .chat_id(chat_id.to_string())
//     //             .text(notification.to_string())
//     //             .build(),
//     //     )
//     //     .await?;
//     // debug!("{resp:#?}");
//     // Ok(())
//     unimplemented!()
// }

#[derive(Clone, Valuable)]
pub struct TelegramConfig {
    pub id: i32,
    pub api_key: String,
    pub chat_id: String,
}
