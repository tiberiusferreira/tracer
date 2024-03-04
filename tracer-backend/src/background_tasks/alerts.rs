use crate::api::state::{AppState, ServiceRuntimeData};
use crate::api::{AppStateError, ServiceInAppStateButNotDBError};
use api_structs::ui::service::alerts::AlertConfig;
use api_structs::ServiceId;
use backtraced_error::{error_chain_to_pretty_formatted, SqlxError};
use chrono::Utc;
use sqlx::PgPool;
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, error, info, instrument};

pub mod checker;
pub mod senders;

#[derive(Debug, Error)]
#[error("AlertError")]
pub enum AlertingError {
    Db(#[from] SqlxError),
    AppStateError(#[from] AppStateError),
}

pub struct ServiceRuntimeDataWithAlert {
    pub service_runtime_data: ServiceRuntimeData,
    pub alert: AlertConfig,
}

pub async fn enrich_service_data_with_alert_config(
    con: &PgPool,
    services_runtime_stats: HashMap<ServiceId, ServiceRuntimeData>,
) -> Result<HashMap<ServiceId, ServiceRuntimeDataWithAlert>, SqlxError> {
    let mut services_runtime_data_with_alert_config = HashMap::new();
    for (service_id, service_runtime_data) in services_runtime_stats {
        let config =
            crate::database::service_initialization::get_service_config(&con, &service_id).await?;
        let Some(service_config) = config else {
            let err = AppStateError::ServiceInAppStateButNotDB(
                ServiceInAppStateButNotDBError::new(&service_id),
            );
            error!("{}", error_chain_to_pretty_formatted(err));
            continue;
        };
        services_runtime_data_with_alert_config.insert(
            service_id,
            ServiceRuntimeDataWithAlert {
                service_runtime_data,
                alert: service_config.alert_config,
            },
        );
    }
    Ok(services_runtime_data_with_alert_config)
}

#[instrument(skip_all)]
pub async fn check_for_alerts_and_send(app_state: &AppState) -> Result<(), AlertingError> {
    let mut services_runtime_stats_guard = app_state.services_runtime_stats.write();
    let services_runtime_stats = services_runtime_stats_guard.clone();
    for service in services_runtime_stats_guard.values_mut() {
        service.last_time_checked_for_alerts = Utc::now().naive_utc();
    }
    drop(services_runtime_stats_guard);
    let services_runtime_data_with_alert_config =
        enrich_service_data_with_alert_config(&app_state.con, services_runtime_stats).await?;
    let Some(notification) =
        checker::check_for_new_notification(services_runtime_data_with_alert_config)
    else {
        info!("empty notifications skipping");
        return Ok(());
    };
    debug!("notification={notification}");
    senders::slack::send_to_slack_and_update_database(&app_state.con, &notification).await?;
    senders::telegram::send_to_telegram_and_update_database(&app_state.con, &notification).await?;
    Ok(())
}
