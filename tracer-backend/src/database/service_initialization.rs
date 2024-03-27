use api_structs::ui::service::alerts::{
    AlertConfig, ServiceWideAlertConfig, TraceWideAlertConfig, TraceWideAlertOverwriteConfig,
};
use api_structs::{ServiceId, TraceName};
use backtraced_error::SqlxError;
use sqlx::PgPool;
use std::collections::HashMap;
use std::ops::DerefMut;
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub service_id: ServiceId,
    pub alert_config: AlertConfig,
}

#[instrument(skip_all)]
pub async fn insert_service_config(con: &PgPool, service_id: &ServiceId) -> Result<(), SqlxError> {
    let mut transaction = con
        .begin()
        .await
        .map_err(|e| SqlxError::from_sqlx_error(e, "initializing transaction for init_service"))?;
    sqlx::query!(
        "insert into service (env, name) values ($1::TEXT, $2::TEXT)",
        service_id.env.to_string(),
        service_id.name
    )
    .execute(transaction.deref_mut())
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, format!("inserting {service_id:?}")))?;
    sqlx::query!(
        "insert into service_wide_alert_config (env, service_name) values ($1::TEXT, $2::TEXT)",
        service_id.env.to_string(),
        service_id.name
    )
    .execute(transaction.deref_mut())
    .await
    .map_err(|e| {
        SqlxError::from_sqlx_error(
            e,
            format!("inserting service_wide_alert_config {service_id:?}"),
        )
    })?;
    sqlx::query!(
        "insert into trace_wide_alert_config (env, service_name) values ($1::TEXT, $2::TEXT)",
        service_id.env.to_string(),
        service_id.name
    )
    .execute(transaction.deref_mut())
    .await
    .map_err(|e| {
        SqlxError::from_sqlx_error(
            e,
            format!("inserting trace_wide_alert_config {service_id:?}"),
        )
    })?;
    transaction.commit().await.map_err(|e| {
        SqlxError::from_sqlx_error(
            e,
            format!("committing transaction for initializing {service_id:?}"),
        )
    })?;
    Ok(())
}
#[instrument(skip_all)]
pub async fn get_service_config(
    con: &PgPool,
    service_id: &ServiceId,
) -> Result<Option<ServiceConfig>, SqlxError> {
    let (service, trace, trace_overwrites) = tokio::try_join!(
        get_service_wide_alert_config(con, &service_id),
        get_trace_wide_alert_config(con, &service_id),
        get_trace_wide_alert_config_overwrite(con, &service_id)
    )?;
    let service = match service {
        None => return Ok(None),
        Some(service) => service,
    };
    Ok(Some(ServiceConfig {
        service_id: service_id.clone(),
        alert_config: AlertConfig {
            service_wide: service,
            trace_wide: trace.expect("trace to exist if service does"),
            service_alert_config_trace_overwrite: trace_overwrites,
        },
    }))
}

#[instrument(skip_all)]
pub async fn get_or_init_service_config(
    con: &PgPool,
    service_id: &ServiceId,
) -> Result<ServiceConfig, SqlxError> {
    let service_config = get_service_config(con, service_id).await?;
    return match service_config {
        None => {
            insert_service_config(con, service_id).await?;
            let service_config = get_service_config(con, service_id)
                .await?
                .expect("service config to exist if just inserted");
            Ok(service_config)
        }
        Some(service_config) => Ok(service_config),
    };
}

#[instrument(skip_all)]
pub async fn get_service_wide_alert_config(
    con: &PgPool,
    service_id: &ServiceId,
) -> Result<Option<ServiceWideAlertConfig>, SqlxError> {
    struct RawServiceWideAlertConfig {
        min_instance_count: i64,
        max_active_traces: i64,
    }
    let raw_service_config: Option<RawServiceWideAlertConfig> = sqlx::query_as!(
        RawServiceWideAlertConfig,
        "select 
            min_instance_count,
            max_active_traces
       from
        service_wide_alert_config
         where env=$1 and service_name=$2;",
        service_id.env.to_string(),
        service_id.name
    )
    .fetch_optional(con)
    .await
    .map_err(|e| {
        SqlxError::from_sqlx_error(
            e,
            format!("getting service_wide_alert_config using {service_id:?}",),
        )
    })?;
    Ok(raw_service_config.map(|e| ServiceWideAlertConfig {
        min_instance_count: e.min_instance_count as u64,
        max_active_traces_count: e.max_active_traces as u64,
    }))
}

#[instrument(skip_all)]
pub async fn get_trace_wide_alert_config(
    con: &PgPool,
    service_id: &ServiceId,
) -> Result<Option<TraceWideAlertConfig>, SqlxError> {
    struct RawTraceWideAlertConfig {
        max_trace_duration_ms: i64,
        max_traces_with_warning_percentage: i64,
        percentage_check_time_window_secs: i64,
        percentage_check_min_number_samples: i64,
    }
    let raw_service_config: Option<RawTraceWideAlertConfig> = sqlx::query_as!(
        RawTraceWideAlertConfig,
        "select
            max_trace_duration_ms,
            max_traces_with_warning_percentage,
            percentage_check_time_window_secs,
            percentage_check_min_number_samples
       from
        trace_wide_alert_config
         where env=$1 and service_name=$2;",
        service_id.env.to_string(),
        service_id.name
    )
    .fetch_optional(con)
    .await
    .map_err(|e| {
        SqlxError::from_sqlx_error(
            e,
            format!("getting trace_wide_alert_config using {service_id:?}",),
        )
    })?;
    Ok(raw_service_config.map(|e| TraceWideAlertConfig {
        max_trace_duration_ms: e.max_trace_duration_ms as u64,
        max_traces_with_warning_percentage: e.max_traces_with_warning_percentage as u64,
        percentage_check_time_window_secs: e.percentage_check_time_window_secs as u64,
        percentage_check_min_number_samples: e.percentage_check_min_number_samples as u64,
    }))
}

#[instrument(skip_all)]
pub async fn get_trace_wide_alert_config_overwrite(
    con: &PgPool,
    service_id: &ServiceId,
) -> Result<HashMap<TraceName, TraceWideAlertOverwriteConfig>, SqlxError> {
    struct RawTraceWideAlertOverwriteConfig {
        top_level_span_name: String,
        max_traces_with_warning_percentage: i64,
        max_trace_duration_ms: i64,
    }
    let raw_service_config: Vec<RawTraceWideAlertOverwriteConfig> = sqlx::query_as!(
        RawTraceWideAlertOverwriteConfig,
        "select
            top_level_span_name,
            max_traces_with_warning_percentage,
            max_trace_duration_ms
       from
        trace_wide_alert_config_overwrite
         where env=$1 and service_name=$2;",
        service_id.env.to_string(),
        service_id.name
    )
    .fetch_all(con)
    .await
    .map_err(|e| {
        SqlxError::from_sqlx_error(
            e,
            format!("getting trace_wide_alert_config using {service_id:?}",),
        )
    })?;
    Ok(raw_service_config
        .into_iter()
        .fold(HashMap::new(), |mut acc, curr| {
            acc.insert(
                curr.top_level_span_name,
                TraceWideAlertOverwriteConfig {
                    max_trace_duration_ms: curr.max_trace_duration_ms as u64,
                    max_traces_with_warning_percentage: curr.max_traces_with_warning_percentage
                        as u64,
                },
            );
            acc
        }))
}
