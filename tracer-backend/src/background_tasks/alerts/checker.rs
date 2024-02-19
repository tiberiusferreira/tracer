use crate::api::state::InstanceState;
use crate::background_tasks::alerts::ServiceRuntimeDataWithAlert;
use crate::MAX_NOTIFICATION_SIZE_CHARS;
use api_structs::time_conversion::{nanos_to_millis, time_from_nanos};
use api_structs::ui::service::alerts::{
    AlertConfig, InstanceWideAlertConfig, ServiceWideAlertConfig, TraceWideAlertConfig,
    TraceWideAlertOverwriteConfig,
};
use api_structs::ui::service::InstanceDataPoint;
use api_structs::{ServiceId, TraceName};
use chrono::NaiveDateTime;
use std::collections::HashMap;
use tracing::{debug, info, instrument};

pub struct ServiceWideAlertChecker {
    min_instance_count_alert: Option<String>,
    max_active_traces_count_alert: Option<String>,
}

impl ServiceWideAlertChecker {
    pub fn alerts(self) -> Vec<String> {
        let mut alerts = vec![];
        if let Some(min_instance_count_alert) = self.min_instance_count_alert {
            alerts.push(min_instance_count_alert);
        }
        if let Some(max_active_traces_count_alert) = self.max_active_traces_count_alert {
            alerts.push(max_active_traces_count_alert);
        }
        alerts
    }
    pub fn new() -> Self {
        Self {
            min_instance_count_alert: None,
            max_active_traces_count_alert: None,
        }
    }
    pub fn check_instance_count(
        &mut self,
        alert_config: &ServiceWideAlertConfig,
        instance_count_hit: u64,
    ) {
        let min_instance_count = alert_config.min_instance_count;
        if instance_count_hit < min_instance_count {
            self.min_instance_count_alert = Some(format!(
                "Hit instance count of {instance_count_hit}, below minimum of {min_instance_count}"
            ));
        }
    }
    pub fn update_active_trace_count(
        &mut self,
        alert_config: &ServiceWideAlertConfig,
        active_trace_count_hit: u64,
    ) {
        let max_active_traces = alert_config.max_active_traces;
        if max_active_traces < active_trace_count_hit {
            self.max_active_traces_count_alert = Some(format!("Hit active traces count of {active_trace_count_hit}, above maximum of {max_active_traces}"));
        }
    }
}

pub struct InstanceWideAlertChecker {
    max_received_spe_alert: Option<String>,
    max_received_trace_kb_alert: Option<String>,
    max_received_orphan_event_kb_alert: Option<String>,
    max_export_buffer_usage_alert: Option<String>,
    orphan_events_per_minute_usage_alert: Option<String>,
    max_orphan_events_dropped_by_sampling_per_min_alert: Option<String>,
    max_spe_dropped_due_to_full_export_buffer_per_min_alert: Option<String>,
}

impl InstanceWideAlertChecker {
    pub fn alerts(self) -> Vec<String> {
        let mut alerts = vec![];
        if let Some(max_received_spe_alert) = self.max_received_spe_alert {
            alerts.push(max_received_spe_alert);
        }
        if let Some(max_received_trace_kb_alert) = self.max_received_trace_kb_alert {
            alerts.push(max_received_trace_kb_alert);
        }
        if let Some(max_received_orphan_event_kb_alert) = self.max_received_orphan_event_kb_alert {
            alerts.push(max_received_orphan_event_kb_alert);
        }
        if let Some(max_export_buffer_usage_alert) = self.max_export_buffer_usage_alert {
            alerts.push(max_export_buffer_usage_alert);
        }
        if let Some(orphan_events_per_minute_usage_alert) =
            self.orphan_events_per_minute_usage_alert
        {
            alerts.push(orphan_events_per_minute_usage_alert);
        }
        if let Some(max_orphan_events_dropped_by_sampling_per_min_alert) =
            self.max_orphan_events_dropped_by_sampling_per_min_alert
        {
            alerts.push(max_orphan_events_dropped_by_sampling_per_min_alert);
        }
        if let Some(max_spe_dropped_due_to_full_export_buffer_per_min_alert) =
            self.max_spe_dropped_due_to_full_export_buffer_per_min_alert
        {
            alerts.push(max_spe_dropped_due_to_full_export_buffer_per_min_alert);
        }
        alerts
    }
    pub fn new() -> Self {
        Self {
            max_received_spe_alert: None,
            max_received_trace_kb_alert: None,
            max_received_orphan_event_kb_alert: None,
            max_export_buffer_usage_alert: None,
            orphan_events_per_minute_usage_alert: None,
            max_orphan_events_dropped_by_sampling_per_min_alert: None,
            max_spe_dropped_due_to_full_export_buffer_per_min_alert: None,
        }
    }
    pub fn update_using_data_point(
        &mut self,
        alert_config: &InstanceWideAlertConfig,
        data_point: &InstanceDataPoint,
    ) {
        let max_received_spe_hit = data_point.received_spe;
        let max_received_spe = alert_config.max_received_spe;
        if max_received_spe < max_received_spe_hit {
            self.max_received_spe_alert = Some(format!(
                "Received {max_received_spe_hit} SpE, above limit of {max_received_spe}"
            ));
        }

        let received_trace_kb_hit = data_point.received_trace_bytes / 1000;
        let max_received_trace_kb = alert_config.max_received_trace_kb;
        if max_received_trace_kb < received_trace_kb_hit {
            self.max_received_trace_kb_alert = Some(format!("Received trace with {received_trace_kb_hit}kb, above limit of {max_received_trace_kb}"));
        }

        let received_orphan_event_kbytes_hit = data_point.received_orphan_event_bytes / 1000;
        let max_received_orphan_event_kb = alert_config.max_received_orphan_event_kb;
        if max_received_orphan_event_kb < received_orphan_event_kbytes_hit {
            self.max_received_orphan_event_kb_alert = Some(format!("Received orphan event with {received_orphan_event_kbytes_hit}kb, above limit of {max_received_orphan_event_kb}"));
        }

        let export_buffer_usage_hit = data_point.tracer_status.spe_buffer_usage;
        let max_export_buffer_usage = alert_config.max_export_buffer_usage;
        if max_export_buffer_usage < export_buffer_usage_hit {
            self.max_export_buffer_usage_alert = Some(format!("Export buffer usage hit {export_buffer_usage_hit}, above limit of {max_export_buffer_usage}"));
        }

        let orphan_events_per_minute_usage_hit =
            data_point.tracer_status.orphan_events_per_minute_usage;
        let orphan_events_per_minute_usage = alert_config.orphan_events_per_minute_usage;
        if orphan_events_per_minute_usage < orphan_events_per_minute_usage_hit {
            self.orphan_events_per_minute_usage_alert = Some(format!("Orphan events per minute usage hit {orphan_events_per_minute_usage_hit}, above limit of {orphan_events_per_minute_usage}"));
        }

        let orphan_events_dropped_by_sampling_per_minute_hit = data_point
            .tracer_status
            .orphan_events_dropped_by_sampling_per_minute;
        let max_orphan_events_dropped_by_sampling_per_min =
            alert_config.max_orphan_events_dropped_by_sampling_per_min;
        if max_orphan_events_dropped_by_sampling_per_min
            < orphan_events_dropped_by_sampling_per_minute_hit
        {
            self.max_orphan_events_dropped_by_sampling_per_min_alert = Some(format!("Orphan events dropped by sampling per minute hit {orphan_events_dropped_by_sampling_per_minute_hit}, above limit of {max_orphan_events_dropped_by_sampling_per_min}"));
        }

        let spe_dropped_due_to_full_export_buffer_per_min_hit = data_point
            .tracer_status
            .spe_dropped_due_to_full_export_buffer_per_min;
        let max_spe_dropped_due_to_full_export_buffer_per_min =
            alert_config.max_spe_dropped_due_to_full_export_buffer_per_min;
        if max_spe_dropped_due_to_full_export_buffer_per_min
            < spe_dropped_due_to_full_export_buffer_per_min_hit
        {
            self.max_spe_dropped_due_to_full_export_buffer_per_min_alert = Some(format!("SpE dropped due to full export buffer hit {spe_dropped_due_to_full_export_buffer_per_min_hit}, above limit of {max_spe_dropped_due_to_full_export_buffer_per_min}"));
        }
    }
}

pub struct TraceWideAlertChecker {
    alerts: Vec<String>,
}

impl TraceWideAlertChecker {
    pub fn alerts(self) -> Vec<String> {
        self.alerts
    }
    pub fn new() -> Self {
        Self { alerts: vec![] }
    }
    fn add_alert_if_not_full(&mut self, alert: String) {
        if self.alerts.len() < 3 {
            self.alerts.push(alert);
        }
    }
    pub fn update(
        &mut self,
        data_point: &InstanceDataPoint,
        trace_wide_alert_config: &TraceWideAlertConfig,
        trace_wide_alert_overwrite_config: &HashMap<TraceName, TraceWideAlertOverwriteConfig>,
    ) {
        for (trace_name, status) in &data_point.tracer_status.per_minute_trace_stats {
            let hit = status.spe_usage_per_minute;
            let max = trace_wide_alert_config.max_traces_dropped_by_sampling_per_min;
            if max < hit {
                self.add_alert_if_not_full(format!(
                    "Trace {trace_name} was dropped {hit} times per minute, above limit of {max}"
                ))
            }
        }
        for t in data_point.active_and_finished_iter() {
            let trace_name = &t.trace_name;
            if let Some(duration) = t.duration {
                let duration_hit_ms = nanos_to_millis(duration);
                let max_duration_ms = trace_wide_alert_overwrite_config
                    .get(&t.trace_name)
                    .map(|d| d.max_trace_duration_ms)
                    .unwrap_or_else(|| trace_wide_alert_config.max_trace_duration_ms);
                if max_duration_ms < duration_hit_ms {
                    self.add_alert_if_not_full(format!("Trace {trace_name} hit duration of {duration_hit_ms}ms, above limit of {max_duration_ms}"));
                }
            }
            if t.new_errors {
                self.add_alert_if_not_full(format!("Trace {trace_name} had errors"));
            }
        }
    }
}

pub fn update_checker_with_instance_data(
    alert_checkers: &mut AlertCheckers,
    alert_config: &AlertConfig,
    instance_data: &InstanceState,
    last_time_checked_for_alerts: NaiveDateTime,
) {
    for data_point in &instance_data.time_data_points {
        // skip data already checked
        if time_from_nanos(data_point.timestamp) < last_time_checked_for_alerts {
            continue;
        }
        alert_checkers.service_wide.update_active_trace_count(
            &alert_config.service_wide,
            data_point.active_traces.len() as u64,
        );
        alert_checkers
            .instance_wide
            .update_using_data_point(&alert_config.instance_wide, data_point);
        alert_checkers.trace_wide.update(
            data_point,
            &alert_config.trace_wide,
            &alert_config.service_alert_config_trace_overwrite,
        );
    }
}

pub struct AlertCheckers {
    pub service_wide: ServiceWideAlertChecker,
    pub instance_wide: InstanceWideAlertChecker,
    pub trace_wide: TraceWideAlertChecker,
}

impl AlertCheckers {
    fn new() -> AlertCheckers {
        Self {
            service_wide: ServiceWideAlertChecker::new(),
            instance_wide: InstanceWideAlertChecker::new(),
            trace_wide: TraceWideAlertChecker::new(),
        }
    }
}

pub fn check_service_for_new_alert(
    service_id: ServiceId,
    service_data: ServiceRuntimeDataWithAlert,
) -> Option<String> {
    let alert_config = service_data.alert;
    let mut alert_checkers = AlertCheckers::new();
    alert_checkers.service_wide.check_instance_count(
        &alert_config.service_wide,
        service_data.service_runtime_data.instances.len() as u64,
    );
    for instance_data in service_data.service_runtime_data.instances.values() {
        update_checker_with_instance_data(
            &mut alert_checkers,
            &alert_config,
            instance_data,
            service_data
                .service_runtime_data
                .last_time_checked_for_alerts,
        );
    }
    let mut alerts = alert_checkers.service_wide.alerts();
    alerts.extend(alert_checkers.instance_wide.alerts());
    alerts.extend(alert_checkers.trace_wide.alerts());
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
                info!("{service_id:?} had new alerts");
                debug!("{alert}");
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
