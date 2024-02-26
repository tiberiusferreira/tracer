use crate::background_tasks::alerts::ServiceRuntimeDataWithAlert;
use crate::MAX_NOTIFICATION_SIZE_CHARS;
use api_structs::ServiceId;
use std::collections::HashMap;
use tracing::{info, instrument};

mod checks;

#[instrument(skip_all)]
pub fn check_service_for_new_alert(
    service_id: ServiceId,
    service_data: ServiceRuntimeDataWithAlert,
) -> Option<String> {
    let alert_config = service_data.alert;
    let mut alerts = vec![];
    if let Some(alert) = checks::instance_count(
        &alert_config.service_wide,
        &service_data.service_runtime_data,
    ) {
        alerts.push(alert);
    }
    if let Some(alert) = checks::max_active_traces(
        &alert_config.service_wide,
        &service_data.service_runtime_data,
    ) {
        alerts.push(alert);
    }
    if let Some(alert) = checks::export_buffer_usage_percentage(
        &alert_config.service_wide,
        &service_data.service_runtime_data,
    ) {
        alerts.push(alert);
    }
    if let Some(alert) = checks::trace_alerts(&alert_config, &service_data.service_runtime_data) {
        alerts.push(alert);
    }
    if let Some(alert) =
        checks::orphan_events_alerts(&alert_config, &service_data.service_runtime_data)
    {
        alerts.push(alert);
    }

    if !alerts.is_empty() {
        let alerts = alerts.join("\n");
        let service_alert = format!("{} at {}:\n{alerts}", service_id.name, service_id.env);
        return Some(service_alert);
    } else {
        None
    }
}

#[instrument(skip_all)]
pub fn check_for_new_notification(
    services_runtime_stats: HashMap<ServiceId, ServiceRuntimeDataWithAlert>,
) -> Option<String> {
    let mut service_alerts = vec![];
    for (service_id, service_data) in services_runtime_stats {
        match check_service_for_new_alert(service_id.clone(), service_data) {
            None => {
                info!("{service_id:?} had no alerts");
            }
            Some(alert) => {
                info!("{service_id:?} had new alerts {alert}");
                service_alerts.push(alert);
            }
        }
    }
    return if service_alerts.is_empty() {
        None
    } else {
        let all_service_alerts = service_alerts.join("\n");
        let final_truncated_alert = all_service_alerts
            .chars()
            .into_iter()
            .take(MAX_NOTIFICATION_SIZE_CHARS)
            .collect::<String>();
        Some(final_truncated_alert)
    };
}
