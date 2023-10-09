use crate::api::{ApiError, AppState, ChangeFilterInternalRequest, ServiceName};
use api_structs::exporter::{
    LiveServiceInstance, Log, ServiceLogRequest, SubscriberEvent, TracerStats,
};
use api_structs::time_conversion::{nanos_to_db_i64, time_from_nanos};
use axum::extract::{Query, State};
use axum::{Json, RequestExt};
use chrono::NaiveDateTime;
use reqwest::StatusCode;
use sqlx::database::HasValueRef;
use sqlx::error::BoxDynError;
use sqlx::{Decode, PgPool, Postgres};
use std::collections::HashMap;
use std::iter::Map;
use std::ops::DerefMut;
use std::vec::IntoIter;
use tracing::{debug, error, instrument, trace};

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "severity_level", rename_all = "lowercase")]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Severity {
    pub fn to_api(&self) -> api_structs::exporter::Severity {
        match self {
            Severity::Trace => api_structs::exporter::Severity::Trace,
            Severity::Debug => api_structs::exporter::Severity::Debug,
            Severity::Info => api_structs::exporter::Severity::Info,
            Severity::Warn => api_structs::exporter::Severity::Warn,
            Severity::Error => api_structs::exporter::Severity::Error,
        }
    }
}
impl From<api_structs::exporter::Severity> for Severity {
    fn from(value: api_structs::exporter::Severity) -> Self {
        match value {
            api_structs::exporter::Severity::Trace => Self::Trace,
            api_structs::exporter::Severity::Debug => Self::Debug,
            api_structs::exporter::Severity::Info => Self::Info,
            api_structs::exporter::Severity::Warn => Self::Warn,
            api_structs::exporter::Severity::Error => Self::Error,
        }
    }
}
impl TryFrom<&str> for Severity {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, ()> {
        match value.to_lowercase().as_str() {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err(()),
        }
    }
}
pub(crate) async fn process_orphan_event(
    con: &PgPool,
    service_name: &str,
    orphan_event: api_structs::exporter::NewOrphanEvent,
) -> Result<(), ApiError> {
    let timestamp = nanos_to_db_i64(orphan_event.timestamp);
    let level = Severity::from(orphan_event.level);
    sqlx::query!(
        "insert into log (timestamp, service_name, severity, value) VALUES ($1::ubigint, $2, $3, $4);",
        timestamp as _,
        service_name as _,
        &level as &Severity,
        orphan_event.name as _
    ).execute(con).await?;
    Ok(())
}

pub(crate) async fn insert_new_trace(
    con: &PgPool,
    service_name: &str,
    service_id: i64,
    new_span: &api_structs::exporter::NewSpan,
) -> Result<(), ApiError> {
    let timestamp = i64::try_from(new_span.timestamp).expect("timestamp to fit i64");
    let trace_id = i64::try_from(new_span.trace_id.get()).expect("id to fit i64");
    sqlx::query!(
        "insert into trace (service_id, id, timestamp, service_name, \
        top_level_span_name, duration, warning_count, has_errors) values \
        ($1, $2, $3, $4, $5, null, 0, false);",
        service_id,
        trace_id as _,
        timestamp as _,
        service_name as _,
        new_span.name as _
    )
    .execute(con)
    .await?;
    Ok(())
}

pub(crate) async fn insert_new_span(
    con: &PgPool,
    service_name: &str,
    service_id: i64,
    new_span: &api_structs::exporter::NewSpan,
) -> Result<(), ApiError> {
    let timestamp = i64::try_from(new_span.timestamp).expect("timestamp to fit i64");
    let trace_id = i64::try_from(new_span.trace_id.get()).expect("id to fit i64");
    let span_id = i64::try_from(new_span.id.get()).expect("id to fit i64");
    let parent_id = new_span
        .parent_id
        .map(|e| i64::try_from(e.get()).expect("id to fit i64"));
    sqlx::query!(
        "insert into span (id, service_id, trace_id, timestamp, parent_id, \
        duration, name) values \
        ($1, $2, $3, $4, $5, null, $6);",
        span_id as _,
        service_id,
        trace_id as _,
        timestamp as _,
        parent_id as _,
        new_span.name as _
    )
    .execute(con)
    .await?;
    Ok(())
}

pub(crate) async fn process_new_span_event(
    con: &PgPool,
    service_id: i64,
    span_event: api_structs::exporter::NewSpanEvent,
) -> Result<(), ApiError> {
    let timestamp = i64::try_from(span_event.timestamp).expect("timestamp to fit i64");
    let span_id = i64::try_from(span_event.span_id.get()).expect("id to fit i64");
    let trace_id = i64::try_from(span_event.trace_id.get()).expect("id to fit i64");
    let level = Severity::from(span_event.level);

    sqlx::query!(
        "insert into event (service_id, trace_id, span_id, timestamp, name, \
        severity) values \
        ($1, $2, $3, $4, $5, $6);",
        service_id,
        trace_id,
        span_id,
        timestamp as _,
        span_event.name as _,
        level as Severity
    )
    .execute(con)
    .await?;
    Ok(())
}

pub(crate) async fn process_closed_span(
    con: &PgPool,
    service_id: i64,
    closed_span: api_structs::exporter::ClosedSpan,
) -> Result<(), ApiError> {
    let id = i64::try_from(closed_span.id.get()).expect("id to fit i64");
    let duration = i64::try_from(closed_span.duration).expect("duration to fit i64");
    let trace_id: i64 = sqlx::query_scalar!(
        "update span set duration=$1 where id=$2 and service_id=$3 returning trace_id",
        duration as _,
        id,
        service_id
    )
    .fetch_one(con)
    .await?;
    if trace_id == id {
        sqlx::query!(
            "update trace set duration=$1 where service_id=$2 and id=$3",
            duration as _,
            service_id,
            id
        )
        .execute(con)
        .await?;
    }
    Ok(())
}
pub(crate) async fn process_new_span(
    con: &PgPool,
    service_name: &str,
    service_id: i64,
    new_span: api_structs::exporter::NewSpan,
) -> Result<(), ApiError> {
    if new_span.id == new_span.trace_id {
        if new_span.parent_id.is_some() {
            error!("Got new span with same id as trace, but had parent id");
            return Err(ApiError {
                code: StatusCode::BAD_REQUEST,
                message: "Got new span with same id as trace, but had parent id".to_string(),
            });
        }
        insert_new_trace(con, service_name, service_id, &new_span).await?;
        insert_new_span(con, service_name, service_id, &new_span).await?;
    } else {
        insert_new_span(con, service_name, service_id, &new_span).await?;
    }
    Ok(())
}

pub async fn process_trace_event(
    con: &PgPool,
    service_name: &str,
    service_id: i64,
    event: SubscriberEvent,
) -> Result<(), ApiError> {
    match event {
        SubscriberEvent::NewSpan(new_span) => {
            process_new_span(&con, &service_name, service_id, new_span).await?;
        }
        SubscriberEvent::NewSpanEvent(event) => {
            process_new_span_event(&con, service_id, event).await?;
        }
        SubscriberEvent::ClosedSpan(closed_span) => {
            process_closed_span(&con, service_id, closed_span).await?;
        }
        SubscriberEvent::NewOrphanEvent(orphan_event) => {
            process_orphan_event(&con, &service_name, orphan_event).await?;
        }
    }
    Ok(())
}

#[axum::debug_handler]
#[instrument(skip_all)]
pub(crate) async fn instances_filter_post(
    State(app_state): State<AppState>,
    Json(new_filter): Json<api_structs::exporter::NewFiltersRequest>,
) -> Result<(), ApiError> {
    let handle = {
        match app_state
            .live_instances
            .see_handle
            .write()
            .get(&new_filter.instance_id)
        {
            None => {
                return Err(ApiError {
                    code: StatusCode::BAD_REQUEST,
                    message: format!("No instance with id: {}", new_filter.instance_id),
                });
            }
            Some(handle) => handle.clone(),
        }
    };
    return match handle
        .send(ChangeFilterInternalRequest {
            filters: new_filter.filters,
        })
        .await
    {
        Ok(_sent) => Ok(()),
        Err(_e) => Err(ApiError {
            code: StatusCode::BAD_REQUEST,
            message: format!(
                "Instance with id {} is no longer connected",
                new_filter.instance_id
            ),
        }),
    };
}

#[axum::debug_handler]
#[instrument(skip_all)]
pub(crate) async fn logs_get(
    service_log_request: Query<ServiceLogRequest>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<Log>>, ApiError> {
    let start_timestamp =
        api_structs::time_conversion::nanos_to_db_i64(service_log_request.start_time);
    let service_name = &service_log_request.service_name;
    // select timestamp, severity, value from log;
    pub struct DbLog {
        pub timestamp: i64,
        pub severity: Severity,
        pub value: String,
    }
    let service_list: Vec<DbLog> = sqlx::query_as!(
        DbLog,
        r#"select timestamp, severity as "severity: Severity", value from log where timestamp>$1 and service_name=$2;"#,
        start_timestamp,
        service_name
    )
    .fetch_all(&app_state.con)
    .await?;

    Ok(Json(
        service_list
            .into_iter()
            .map(|e| Log {
                timestamp: api_structs::time_conversion::db_i64_to_nanos(e.timestamp),
                severity: e.severity.to_api(),
                value: e.value,
            })
            .collect(),
    ))
}
#[instrument(skip_all)]
pub(crate) async fn logs_service_names_get(
    State(app_state): State<AppState>,
) -> Result<Json<api_structs::exporter::ServiceNameList>, ApiError> {
    let service_list: Vec<String> =
        sqlx::query_scalar!("select distinct log.service_name from log;")
            .fetch_all(&app_state.con)
            .await?;
    debug!("Got {} services", service_list.len());
    trace!("Got services: {:#?}", service_list);
    Ok(Json(service_list))
}

#[axum::debug_handler]
#[instrument(skip_all)]
pub(crate) async fn instances_get(
    State(app_state): State<AppState>,
) -> Result<Json<api_structs::exporter::LiveInstances>, ApiError> {
    let instances: HashMap<ServiceName, Vec<LiveServiceInstance>> = {
        trace!("cleaning up old instances");
        let mut instances = app_state.live_instances.trace_data.write();
        let instances = instances.deref_mut();

        for (_service_name, live_services) in &mut *instances {
            live_services.retain(|l| {
                let last_seen = time_from_nanos(l.last_seen_timestamp);
                let now = chrono::Utc::now().naive_utc();
                let last_seen_minutes_ago = (now - last_seen).num_minutes();
                trace!("instance {l:?} last seen {last_seen_minutes_ago} minutes ago");
                if last_seen_minutes_ago > 1 {
                    trace!("removing instance last seen {last_seen_minutes_ago} minutes ago");
                    false
                } else {
                    true
                }
            })
        }
        instances.retain(|service_name, instances| !instances.is_empty());
        instances.clone()
    };
    Ok(Json(api_structs::exporter::LiveInstances { instances }))
}

#[instrument(skip_all)]
pub(crate) async fn collector_trace_data_post(
    State(app_state): State<AppState>,
    Json(trace_data): Json<api_structs::exporter::ExportedServiceTraceData>,
) -> Result<(), ApiError> {
    {
        let mut instances = app_state.live_instances.trace_data.write();
        let entry = instances
            .entry(trace_data.service_name.to_string())
            .or_default();
        let new = LiveServiceInstance {
            last_seen_timestamp: api_structs::time_conversion::now_nanos_u64(),
            service_id: trace_data.service_id,
            service_name: trace_data.service_name.to_string(),
            filters: trace_data.filters,
            tracer_stats: trace_data.tracer_stats,
        };
        match entry
            .iter_mut()
            .find(|i| i.service_id == trace_data.service_id)
        {
            None => {
                entry.push(new);
            }
            Some(existing) => {
                *existing = new;
            }
        }
    }

    for event in trace_data.events {
        if let Err(e) = process_trace_event(
            &app_state.con,
            &trace_data.service_name,
            trace_data.service_id,
            event,
        )
        .await
        {
            error!("error processing trace event: {:?}", e);
        }
    }
    Ok(())
}
