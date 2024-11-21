use api_structs::ui::service::alerts::{
    AlertConfig, ServiceWideAlertConfig, TraceWideAlertConfig, TraceWideAlertOverwriteConfig,
};
use api_structs::{ServiceId, TraceName};
use serde::Deserialize;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashMap;
use std::ops::DerefMut;
use std::panic::Location;
use tracing::{instrument, warn};
use tracked_error::SqlxError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("service exists, but missing configuration. Location: {location}")]
    InvalidServiceConfig {
        missing_config: String,
        location: &'static Location<'static>,
    },
    #[error("Database error")]
    Database(#[from] SqlxError),
}

impl From<sqlx::Error> for Error {
    #[track_caller]
    fn from(value: sqlx::Error) -> Self {
        Self::Database(SqlxError::from(value))
    }
}

pub type ServiceDbId = i32;
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub service_db_id: ServiceDbId,
    pub alert_config: AlertConfig,
}

#[instrument(skip_all)]
pub async fn insert_service_and_config(
    transaction: &mut Transaction<'static, Postgres>,
    service_id: &ServiceId,
) -> Result<ServiceDbId, SqlxError> {
    let service_db_id = sqlx::query_scalar!(
        "insert into service (env, name) values ($1::TEXT, $2::TEXT) returning id",
        service_id.env.to_string(),
        service_id.name
    )
    .fetch_one(transaction.deref_mut())
    .await?;
    sqlx::query!(
        "insert into service_wide_alert_config (service_id) values ($1)",
        service_db_id
    )
    .execute(transaction.deref_mut())
    .await?;
    sqlx::query!(
        "insert into trace_wide_alert_config (service_id) values ($1)",
        service_db_id
    )
    .execute(transaction.deref_mut())
    .await?;
    Ok(service_db_id)
}

#[instrument(skip_all)]
pub async fn get_service_config(
    tx: &mut Transaction<'static, Postgres>,
    service_id: ServiceDbId,
) -> Result<ServiceConfig, Error> {
    let service = get_service_wide_alert_config(&mut *tx, service_id).await?;
    let trace = get_trace_wide_alert_config(&mut *tx, service_id).await?;
    let trace_overwrites = get_trace_wide_alert_config_overwrite(&mut *tx, service_id).await?;
    let Some(service_config) = service else {
        return Err(Error::InvalidServiceConfig {
            missing_config: "service_wide_alert_config".to_string(),
            location: Location::caller(),
        });
    };
    let Some(trace_config) = trace else {
        return Err(Error::InvalidServiceConfig {
            missing_config: "trace_wide_alert_config".to_string(),
            location: Location::caller(),
        });
    };

    Ok(ServiceConfig {
        service_db_id: service_id,
        alert_config: AlertConfig {
            service_wide: service_config,
            trace_wide: trace_config,
            service_alert_config_trace_overwrite: trace_overwrites,
        },
    })
}

#[instrument(skip_all)]
pub async fn get_service_db_id(
    con: &mut Transaction<'static, Postgres>,
    service_id: &ServiceId,
) -> Result<Option<i32>, SqlxError> {
    let service_id = sqlx::query_scalar!(
        "select id from service where env=$1 and name=$2 for share",
        service_id.env.to_string(),
        service_id.name
    )
    .fetch_optional(&mut **con)
    .await?;
    Ok(service_id)
}

#[instrument(skip_all)]
pub async fn get_service_alerts(
    con: &mut Transaction<'static, Postgres>,
    service_id: &ServiceId,
) -> Result<Option<i32>, SqlxError> {
    let service_id = sqlx::query_scalar!(
        "select id from service where env=$1 and name=$2 for share",
        service_id.env.to_string(),
        service_id.name
    )
    .fetch_optional(&mut **con)
    .await?;
    Ok(service_id)
}

#[instrument(skip_all)]
pub async fn get_or_init_service_config(
    tx: &mut Transaction<'static, Postgres>,
    service_id: &ServiceId,
) -> Result<ServiceConfig, Error> {
    let service_db_id = get_service_db_id(&mut *tx, service_id).await?;
    let service_db_id = match service_db_id {
        None => insert_service_and_config(&mut *tx, service_id).await?,
        Some(service_db_id) => service_db_id,
    };
    let config = get_service_config(&mut *tx, service_db_id).await?;
    Ok(config)
}

// #[instrument(skip_all)]
// pub async fn get_or_create_service(
//     con: &PgPool,
//     service_id: &ServiceId,
// ) -> Result<ServiceDbId, SqlxError> {
// }

#[instrument(skip_all)]
pub async fn get_service_wide_alert_config(
    tx: &mut Transaction<'static, Postgres>,
    service_db_id: ServiceDbId,
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
         where service_id=$1;",
        service_db_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(raw_service_config.map(|e| ServiceWideAlertConfig {
        min_instance_count: e.min_instance_count as u64,
        max_active_traces_count: e.max_active_traces as u64,
    }))
}

#[instrument(skip_all)]
pub async fn get_trace_wide_alert_config(
    tx: &mut Transaction<'static, Postgres>,
    service_id: ServiceDbId,
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
         where service_id=$1;",
        service_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(raw_service_config.map(|e| TraceWideAlertConfig {
        max_trace_duration_ms: e.max_trace_duration_ms as u64,
        max_traces_with_warning_percentage: e.max_traces_with_warning_percentage as u64,
        percentage_check_time_window_secs: e.percentage_check_time_window_secs as u64,
        percentage_check_min_number_samples: e.percentage_check_min_number_samples as u64,
    }))
}

#[instrument(skip_all)]
pub async fn get_trace_wide_alert_config_overwrite(
    tx: &mut Transaction<'static, Postgres>,
    service_id: ServiceDbId,
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
         where service_id=$1;",
        service_id
    )
    .fetch_all(&mut **tx)
    .await?;
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
