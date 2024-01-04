use crate::api::handlers::Severity;
use api_structs::exporter::trace_exporting::NewSpanEvent;
use api_structs::ui::service_health::{
    AlertConfig, ServiceAlertConfigTraceOverwrite, TraceAlertConfig,
};
use api_structs::Env;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use thiserror::Error;
use tracing::{error, info, instrument, trace};

#[derive(Debug, Error)]
enum DatabaseError {
    #[error("Database error")]
    Sqlx {
        // backtrace: std::backtrace::Backtrace,
        #[from]
        source: sqlx::Error,
    },
}

struct RawServiceConfig {
    service_name: String,
    max_export_buffer_usage: i64,
    max_orphan_events_per_min: i64,
    max_orphan_events_dropped_by_sampling_per_min: i64,
    max_spe_dropped_due_to_full_export_buffer: i64,
    max_traces_dropped_by_sampling_per_min: i64,
    min_instance_count: i64,
    max_active_traces: i64,
    max_received_spe: i64,
    max_received_trace_kb: i64,
    max_received_orphan_event_kb: i64,
    max_trace_duration_ms: i64,
    max_traces_with_warning_percentage: i64,
    max_traces_with_error_percentage: i64,
    percentage_check_time_window_secs: i64,
    percentage_check_min_number_samples: i64,
    min_alert_period_seconds: i64,
    alert_url: Option<String>,
}
#[instrument(skip_all)]
pub(crate) async fn get_service_raw_config(
    con: &PgPool,
    service_name: &str,
    env: Env,
) -> Result<Option<RawServiceConfig>, sqlx::Error> {
    let raw_service_config = sqlx::query_as!(
        RawServiceConfig,
        "select 
            service_name,
            max_export_buffer_usage,
            max_orphan_events_per_min,
            max_orphan_events_dropped_by_sampling_per_min,
            max_spe_dropped_due_to_full_export_buffer,
            max_traces_dropped_by_sampling_per_min,
            min_instance_count,
            max_active_traces,
            max_received_spe,
            max_received_trace_kb,
            max_received_orphan_event_kb,
            max_trace_duration_ms,
            max_traces_with_warning_percentage,
            max_traces_with_error_percentage,
            percentage_check_time_window_secs,
            percentage_check_min_number_samples,
            min_alert_period_seconds,
            alert_url
       from
        service_alert_config
         where service_name=$1;",
        format!("{}-{}", service_name, env)
    )
    .fetch_optional(con)
    .await?;
    Ok(raw_service_config)
}
#[instrument(skip_all)]
pub async fn get_or_init_service_alert_config(
    con: &PgPool,
    service_name: &str,
    env: Env,
) -> Result<AlertConfig, sqlx::Error> {
    let raw_service_config = get_service_raw_config(con, &service_name, env).await?;
    let raw_service_config = match raw_service_config {
        None => {
            sqlx::query!(
                "insert into service_alert_config (service_name) values ($1::TEXT);",
                &format!("{}-{}", service_name, env)
            )
            .execute(con)
            .await?;
            let raw_service_config = get_service_raw_config(con, &service_name, env)
                .await?
                .expect("to exist, just inserted it");
            raw_service_config
        }
        Some(existing) => existing,
    };
    Ok(AlertConfig {
        service_alert_config: api_structs::ui::service_health::ServiceAlertConfig {
            max_export_buffer_usage: raw_service_config.max_export_buffer_usage as u64,
            max_orphan_events_per_min: raw_service_config.max_export_buffer_usage as u64,
            max_orphan_events_dropped_by_sampling_per_min: raw_service_config
                .max_orphan_events_dropped_by_sampling_per_min
                as u64,
            max_spe_dropped_due_to_full_export_buffer: raw_service_config
                .max_spe_dropped_due_to_full_export_buffer
                as u64,
            min_instance_count: raw_service_config.min_instance_count as u64,
            max_active_traces: raw_service_config.max_active_traces as u64,
            max_received_spe: raw_service_config.max_received_spe as u64,
            max_received_trace_kb: raw_service_config.max_received_trace_kb as u64,
            max_received_orphan_event_kb: raw_service_config.max_received_orphan_event_kb as u64,
            percentage_check_time_window_secs: raw_service_config.percentage_check_time_window_secs
                as u64,
            percentage_check_min_number_samples: raw_service_config
                .percentage_check_min_number_samples
                as u64,
            trace_alert_config: TraceAlertConfig {
                max_trace_duration: raw_service_config.max_trace_duration_ms as u64,
                max_traces_with_warning_percentage: raw_service_config
                    .max_traces_with_warning_percentage
                    as u64,
                max_traces_dropped_by_sampling_per_min: raw_service_config
                    .max_traces_dropped_by_sampling_per_min
                    as u64,
                max_traces_with_error_percentage: raw_service_config
                    .max_traces_with_error_percentage
                    as u64,
            },
            min_alert_period_seconds: raw_service_config.min_alert_period_seconds as u64,
            alert_url: raw_service_config.alert_url,
        },
        service_alert_config_trace_overwrite: ServiceAlertConfigTraceOverwrite {
            trace_to_overwrite_config: HashMap::new(),
        },
    })
}

#[instrument(skip_all)]
pub(crate) async fn insert_events(
    con: &mut Transaction<'static, Postgres>,
    new_events: &[NewSpanEvent],
    trace_id: u64,
    service_id: i64,
    relocated_event_vec_indexes: &HashSet<usize>,
) -> Result<(), sqlx::Error> {
    if new_events.is_empty() {
        info!("No new trace events to insert");
        return Ok(());
    } else {
        info!("Inserting {} events", new_events.len());
    }
    for e in new_events {
        trace!("Inserting event: {:?}", e);
    }
    let service_ids: Vec<i64> = new_events.iter().map(|_s| service_id).collect();
    let trace_ids: Vec<i64> = new_events.iter().map(|_s| trace_id as i64).collect();
    let timestamps: Vec<i64> = new_events.iter().map(|s| s.timestamp as i64).collect();
    let relocateds: Vec<bool> = new_events
        .iter()
        .enumerate()
        .map(|(idx, _e)| relocated_event_vec_indexes.contains(&idx))
        .collect();
    let span_ids: Vec<i64> = new_events.iter().map(|s| s.span_id as i64).collect();
    let names: Vec<Option<String>> = new_events.iter().map(|s| s.message.clone()).collect();
    let severities: Vec<Severity> = new_events.iter().map(|s| Severity::from(s.level)).collect();
    let db_event_ids: Vec<i64> = match sqlx::query_scalar!(
            "insert into event (service_id, trace_id, span_id, timestamp, message, severity, relocated)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::TEXT[], $6::severity_level[], $7::BOOLEAN[]) returning id;",
            &service_ids,
            &trace_ids,
            &span_ids,
            &timestamps,
            &names as &Vec<Option<String>>,
            severities.as_slice() as &[Severity],
            &relocateds,
        )
        .fetch_all(con.deref_mut())
        .await{
        Ok(ids) => {
            info!(inserted_events_count=ids.len(),"Inserted events");
            ids
        }
        Err(e) => {
            error!("Error when inserting events");
            error!("service_ids={:?}", service_ids);
            error!("trace_id={:?}", trace_id);
            error!("span_ids={:?}", span_ids);
            error!("timestamps={:?}", timestamps);
            for name in names{
                error!("name={:?}", name);
            }
            error!("severities={:?}", severities);
            error!("relocateds={:?}", relocateds);
            return Err(e);
        }
    };

    let mut kv_service_id = vec![];
    let mut kv_trace_id = vec![];
    let mut kv_span_id = vec![];
    let mut kv_event_id = vec![];
    let mut kv_key = vec![];
    let mut kv_value = vec![];
    let mut kv_timestamp = vec![];
    for (idx, span) in new_events.iter().enumerate() {
        for (key, val) in &span.key_vals {
            kv_service_id.push(service_id);
            kv_trace_id.push(trace_id as i64);
            kv_span_id.push(span.span_id as i64);
            kv_event_id.push(db_event_ids[idx]);
            kv_key.push(key.as_str());
            kv_value.push(val.as_str());
            kv_timestamp.push(span.timestamp as i64);
        }
    }
    if kv_service_id.is_empty() {
        info!("No event key-values to insert");
        return Ok(());
    } else {
        info!("Inserting {} event key-values", kv_service_id.len());
    }
    match sqlx::query!(
            "insert into event_key_value (service_id, trace_id, span_id, event_id, key, value, timestamp)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::TEXT[], $6::TEXT[], $7::BIGINT[]);",
            &kv_service_id,
            &kv_trace_id,
            &kv_span_id,
            &kv_event_id,
            &kv_key as &Vec<&str>,
            &kv_value as &Vec<&str>,
            &kv_timestamp
        )
        .execute(con.deref_mut())
        .await {
        Ok(_) => {
            info!("Inserted span key-values");
        },
        Err(e) => {
            error!("Error when inserting span key-values");
            error!("kv_service_id: {:?}", kv_service_id);
            error!("kv_trace_id: {:?}", kv_trace_id);
            error!("kv_span_id: {:?}", kv_span_id);
            error!("kv_timestamp: {:?}", kv_timestamp);
            error!("kv_key: {:?}", kv_key);
            error!("kv_value: {:?}", kv_value);
            return Err(e)
        }
    };

    Ok(())
}
