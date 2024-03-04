use crate::api::state::ServiceRuntimeData;
use api_structs::time_conversion::{nanos_to_millis, time_from_nanos};
use api_structs::ui::service::alerts::{AlertConfig, ServiceWideAlertConfig};
use api_structs::ui::service::TraceHeader;
use chrono::{NaiveDateTime, Utc};
use tracing::log::info;
use tracing::{debug, instrument, trace};

pub fn instance_count(
    alert_config: &ServiceWideAlertConfig,
    current_instance_count: &ServiceRuntimeData,
) -> Option<String> {
    let current_instance_count = current_instance_count.instances.len() as u64;
    let min_instance_count = alert_config.min_instance_count;
    return if current_instance_count < alert_config.min_instance_count {
        Some(format!(
            "Hit instance count of {current_instance_count}, below minimum of {min_instance_count}"
        ))
    } else {
        None
    };
}

#[instrument(skip_all)]
pub fn max_active_traces(
    alert_config: &ServiceWideAlertConfig,
    service_runtime_data: &ServiceRuntimeData,
) -> Option<String> {
    let max_active_traces_count = alert_config.max_active_traces_count;
    debug!("max_active_traces_count={}", max_active_traces_count);
    for data_point in service_runtime_data.data_points_since_last_alert_check_reversed() {
        trace!("checking {:?}", data_point.active_traces);
        let current_active_traces_count = data_point.active_traces.len();
        debug!("active_traces_count={}", current_active_traces_count);
        if max_active_traces_count < current_active_traces_count as u64 {
            let event_datetime = time_from_nanos(data_point.timestamp);
            let now = Utc::now().naive_utc();
            let seconds_ago = (now - event_datetime).num_seconds();
            debug!("reporting with event_datetime={event_datetime} now={now} seconds_ago={seconds_ago}");
            return Some(format!(
                "Too many active traces ({current_active_traces_count}), above maximum of {max_active_traces_count} {seconds_ago} seconds ago ({event_datetime})",
            ));
        }
    }
    None
}

#[instrument(skip_all)]
pub fn export_buffer_usage_percentage(
    alert_config: &ServiceWideAlertConfig,
    service_runtime_data: &ServiceRuntimeData,
) -> Option<String> {
    let max_export_buffer_usage_percentage = alert_config.max_export_buffer_usage_percentage;
    debug!(
        "max_export_buffer_usage_percentage={}",
        max_export_buffer_usage_percentage
    );
    for data_point in service_runtime_data.data_points_since_last_alert_check_reversed() {
        trace!("checking {:?}", data_point.export_buffer_stats);
        let export_buffer_usage = data_point.export_buffer_stats.export_buffer_usage;
        let export_buffer_capacity = data_point.export_buffer_stats.export_buffer_capacity;
        let current_export_buffer_usage_percentage_0_to_100 =
            data_point.export_buffer_stats.usage_percentage_0_to_100();
        debug!(
            "current_export_buffer_usage_percentage_0_to_100 {:.2}",
            current_export_buffer_usage_percentage_0_to_100
        );
        if max_export_buffer_usage_percentage
            < current_export_buffer_usage_percentage_0_to_100 as u64
        {
            let event_datetime = time_from_nanos(data_point.timestamp);
            let now = Utc::now().naive_utc();
            let seconds_ago = (now - event_datetime).num_seconds();
            debug!("reporting with event_datetime={event_datetime} now={now} seconds_ago={seconds_ago} export_buffer_usage={export_buffer_usage} export_buffer_capacity={export_buffer_capacity}");
            return Some(format!(
                "Export buffer usage hit {current_export_buffer_usage_percentage_0_to_100}% ({export_buffer_usage}/{export_buffer_capacity}), above maximum of {max_export_buffer_usage_percentage}% {seconds_ago} seconds ago ({event_datetime})",
            ));
        }
    }
    None
}

pub fn trace_over_duration_limit(max_duration_ms: u64, trace: &TraceHeader) -> bool {
    let current_duration_nanos = trace.duration_so_far_nanos();
    let current_duration_ms = nanos_to_millis(current_duration_nanos);
    max_duration_ms < current_duration_ms
}

#[derive(Debug, Clone)]
struct EventDateTimeUtc {
    event_datetime_utc: NaiveDateTime,
    seconds_ago: i64,
}
fn event_datetime_utc(timestamp: u64) -> EventDateTimeUtc {
    let event_datetime = time_from_nanos(timestamp);
    let now = Utc::now().naive_utc();
    let seconds_ago = (now - event_datetime).num_seconds();
    EventDateTimeUtc {
        event_datetime_utc: event_datetime,
        seconds_ago,
    }
}

fn create_over_duration_message(
    trace_name: &str,
    trace_timestamp: u64,
    trace_id: u64,
    current_duration_ms: u64,
    max_duration_ms: u64,
) -> String {
    let trace_datetime = event_datetime_utc(trace_timestamp);
    debug!("{trace_datetime:?}");
    let EventDateTimeUtc {
        event_datetime_utc,
        seconds_ago,
    } = trace_datetime;
    format!("Trace {trace_name} (id={trace_id}) hit duration of {current_duration_ms}ms, over maximum of {max_duration_ms}ms {seconds_ago} seconds ago ({event_datetime_utc})")
}

fn create_had_errors_message(trace_name: &str, trace_timestamp: u64, trace_id: u64) -> String {
    let trace_datetime = event_datetime_utc(trace_timestamp);
    debug!("{trace_datetime:?}");
    let EventDateTimeUtc {
        event_datetime_utc,
        seconds_ago,
    } = trace_datetime;
    format!("Trace {trace_name} (id={trace_id}) had errors {seconds_ago} seconds ago ({event_datetime_utc})")
}

fn create_orphan_error_message(timestamp: u64, orphan_event_msg: &str) -> String {
    let trace_datetime = event_datetime_utc(timestamp);
    debug!("{trace_datetime:?}");
    let EventDateTimeUtc {
        event_datetime_utc,
        seconds_ago,
    } = trace_datetime;
    let orphan_event_msg_trimmed = if orphan_event_msg.len() > 20 {
        format!(
            "{}...",
            orphan_event_msg.chars().take(20).collect::<String>()
        )
    } else {
        orphan_event_msg.to_string()
    };
    format!("Had Error Orphan Event {orphan_event_msg_trimmed} {seconds_ago} seconds ago ({event_datetime_utc})")
}

#[instrument(skip_all)]
pub fn orphan_events_alerts(
    alert_config: &AlertConfig,
    service_runtime_data: &ServiceRuntimeData,
) -> Option<String> {
    trace!("alert_config={alert_config:?}");
    let mut alerts = vec![];
    for data_point in service_runtime_data.data_points_since_last_alert_check_reversed() {
        for orphan_event in &data_point.orphan_events {
            if alerts.len() > 5 {
                info!("Got 5 alerts, skipping rest");
                break;
            }
            trace!("checking: {orphan_event:?}");
            if matches!(orphan_event.severity, api_structs::Severity::Error) {
                let message = orphan_event
                    .message
                    .as_ref()
                    .map(|e| e.as_str())
                    .unwrap_or("");
                let alert = create_orphan_error_message(orphan_event.timestamp, message);
                alerts.push(alert);
                debug!("orphan event with error: {orphan_event:?}");
            }
        }
    }
    if !alerts.is_empty() {
        info!("alerts={alerts:#?}");
        Some(alerts.join("\n"))
    } else {
        info!("No Alerts");
        None
    }
}
#[instrument(skip_all)]
pub fn trace_alerts(
    alert_config: &AlertConfig,
    service_runtime_data: &ServiceRuntimeData,
) -> Option<String> {
    trace!("alert_config={alert_config:?}");
    let mut alerts = vec![];
    for data_point in service_runtime_data.data_points_since_last_alert_check_reversed() {
        for trace in data_point.active_and_finished_iter() {
            if alerts.len() > 5 {
                info!("Got 5 alerts, skipping rest");
                break;
            }
            trace!("checking: {trace:?}");
            let max_duration_ms = match alert_config
                .service_alert_config_trace_overwrite
                .get(&trace.trace_name)
            {
                None => {
                    trace!("no overwrite found");
                    alert_config.trace_wide.max_trace_duration_ms
                }
                Some(overwrite) => {
                    trace!("overwrite found: {overwrite:?}");
                    overwrite.max_trace_duration_ms
                }
            };

            debug!(
                "checking trace_name={} duration={}ms max_duration_ms={max_duration_ms}",
                trace.trace_name,
                nanos_to_millis(trace.duration_so_far_nanos())
            );
            if trace_over_duration_limit(max_duration_ms, trace) {
                debug!("over duration limit");
                let over_duration_alert = create_over_duration_message(
                    &trace.trace_name,
                    trace.trace_timestamp,
                    trace.trace_id,
                    nanos_to_millis(trace.duration_so_far_nanos()),
                    max_duration_ms,
                );
                alerts.push(over_duration_alert);
            }
            if trace.new_errors {
                debug!("had errors");
                let errors_alert = create_had_errors_message(
                    &trace.trace_name,
                    trace.trace_timestamp,
                    trace.trace_id,
                );
                alerts.push(errors_alert);
            }
        }
    }
    if !alerts.is_empty() {
        info!("alerts={alerts:#?}");
        Some(alerts.join("\n"))
    } else {
        info!("No Alerts");
        None
    }
}
