use crate::api::{ApiError, AppState, ChangeFilterInternalRequest, ServiceName};
use api_structs::exporter::{
    LiveServiceInstance, Log, NewOrphanEvent, NewSpan, NewSpanEvent, ServiceLogRequest,
};
use api_structs::time_conversion::now_nanos_u64;
use axum::extract::{Query, State};
use axum::routing::trace;
use axum::Json;
use reqwest::StatusCode;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use tracing::{debug, error, instrument, trace, warn};

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "severity_level", rename_all = "lowercase")]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
impl sqlx::postgres::PgHasArrayType for Severity {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("_severity_level")
    }
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

pub fn nanos_to_db_i64(nanos: u64) -> Result<i64, ApiError> {
    i64::try_from(nanos).map_err(|_| ApiError {
        code: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("Error converting nanos {nanos} to i64"),
    })
}
pub fn db_i64_to_nanos(db_i64: i64) -> Result<u64, ApiError> {
    u64::try_from(db_i64).map_err(|_| ApiError {
        code: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("Error converting db_i64 {db_i64} to u64"),
    })
}

#[instrument(skip_all)]
pub(crate) async fn logs_get(
    service_log_request: Query<ServiceLogRequest>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<Log>>, ApiError> {
    let from_timestamp = nanos_to_db_i64(service_log_request.from_date_unix)?;
    let to_timestamp = nanos_to_db_i64(service_log_request.to_date_unix)?;
    let service_name = &service_log_request.service_name;
    pub struct DbLog {
        pub timestamp: i64,
        pub severity: Severity,
        pub value: String,
    }
    let service_list: Vec<DbLog> = sqlx::query_as!(
        DbLog,
        r#"select timestamp, severity as "severity: Severity", value from log
         where timestamp >= $1 and timestamp <= $2 and service_name=$3 order by timestamp desc limit 100000;"#,
        from_timestamp,
        to_timestamp,
        service_name
    )
    .fetch_all(&app_state.con)
    .await?;

    Ok(Json(
        service_list
            .into_iter()
            .map(|e| Log {
                timestamp: db_i64_to_nanos(e.timestamp).expect("db timestamp to fit u64"),
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
    let instances: HashMap<ServiceName, Vec<LiveServiceInstance>> =
        app_state.live_instances.trace_data.read().deref().clone();
    Ok(Json(api_structs::exporter::LiveInstances { instances }))
}

fn update_instance_data(
    app_state: &AppState,
    service_data: &api_structs::exporter::ExportedServiceTraceData,
) {
    let mut instances = app_state.live_instances.trace_data.write();
    let entry = instances
        .entry(service_data.service_name.to_string())
        .or_default();
    let new = LiveServiceInstance {
        last_seen_timestamp: api_structs::time_conversion::now_nanos_u64(),
        service_id: service_data.service_id,
        service_name: service_data.service_name.to_string(),
        filters: service_data.filters.clone(),
        tracer_stats: service_data.tracer_stats.clone(),
    };
    // there won't be many instances of the same service
    match entry
        .iter_mut()
        .find(|i| i.service_id == service_data.service_id)
    {
        None => {
            entry.push(new);
        }
        Some(existing) => {
            *existing = new;
        }
    }
}

/// We should insert a new trace if it doesn't already exist

struct TraceHeader {
    duration: Option<u64>,
}
struct RawTraceHeader {
    duration: Option<i64>,
}
async fn get_db_trace(
    con: &PgPool,
    service_id: i64,
    trace_id: u64,
) -> Result<Option<TraceHeader>, ApiError> {
    let raw: Option<RawTraceHeader> = sqlx::query_as!(
        RawTraceHeader,
        "select duration from trace where service_id=$1 and id=$2",
        service_id as i64,
        trace_id as i64
    )
    .fetch_optional(con)
    .await?;
    return match raw {
        None => Ok(None),
        Some(raw) => Ok(Some(TraceHeader {
            duration: raw.duration.map(|e| e as u64),
        })),
    };
}

#[derive(Debug, Clone)]
struct KnownAndUnknownIds {
    known_span_ids: HashSet<u64>,
    unknown_span_ids: HashSet<u64>,
}

fn check_event_span_references(
    known_and_unknown_ids: &mut KnownAndUnknownIds,
    events: &[NewSpanEvent],
) {
    for e in events {
        if !known_and_unknown_ids.known_span_ids.contains(&e.span_id) {
            known_and_unknown_ids.unknown_span_ids.insert(e.span_id);
        }
    }
}
fn check_span_references(span: &[NewSpan]) -> KnownAndUnknownIds {
    let mut known_span_ids = HashSet::new();
    let mut unknown_span_ids = HashSet::new();
    for s in span {
        if let Some(parent_id) = s.parent_id {
            if !known_span_ids.contains(&parent_id) {
                unknown_span_ids.insert(parent_id);
            }
        }
        known_span_ids.insert(s.id);
    }
    KnownAndUnknownIds {
        known_span_ids,
        unknown_span_ids,
    }
}

async fn check_span_ids_exist_in_db_returning_missing(
    con: &PgPool,
    span_ids_to_check: &HashSet<u64>,
    trace_id: u64,
    service_id: i64,
) -> Result<HashSet<u64>, ApiError> {
    let as_vec: Vec<i64> = span_ids_to_check.iter().map(|e| *e as i64).collect();
    let res: Vec<i64> = sqlx::query_scalar!(
        "select id from span where trace_id=$1 and service_id=$2 and id = ANY($3::BIGINT[])",
        trace_id as i64,
        service_id,
        as_vec.as_slice()
    )
    .fetch_all(con)
    .await?;
    let existing_ids: HashSet<u64> = res.iter().map(|id| *id as u64).collect();
    let missing_ids: HashSet<u64> = span_ids_to_check
        .difference(&existing_ids)
        .cloned()
        .collect();
    Ok(missing_ids)
}

async fn insert_new_trace(
    con: &mut Transaction<'static, Postgres>,
    service_id: i64,
    trace_id: u64,
    service_name: &str,
    top_level_span_name: &str,
    timestamp: u64,
) -> Result<(), ApiError> {
    sqlx::query!(
        "insert into trace (service_id, id, service_name, timestamp, top_level_span_name, 
                    updated_at) values (
                    $1, $2, $3, $4, $5, $6);",
        service_id as _,
        trace_id as i64,
        service_name as _,
        timestamp as i64,
        top_level_span_name as _,
        now_nanos_u64() as i64
    )
    .execute(con.deref_mut())
    .await?;
    Ok(())
}

fn relocate_event_references_from_lost_spans_to_root(
    events: &mut Vec<NewSpanEvent>,
    lost_span_ids: &HashSet<u64>,
    relocated_event_vec_indexes: &mut HashSet<usize>,
    relocate_to: u64,
) {
    for (idx, e) in events.iter_mut().enumerate() {
        if lost_span_ids.contains(&e.span_id) {
            relocated_event_vec_indexes.insert(idx);
            e.span_id = relocate_to;
        }
    }
}
fn relocate_span_references_from_lost_spans_to_root(
    spans: &mut Vec<NewSpan>,
    lost_span_ids: &HashSet<u64>,
    relocated_span_ids: &mut HashSet<u64>,
    relocate_to: u64,
) {
    for s in spans {
        if let Some(parent_id) = s.parent_id {
            if lost_span_ids.contains(&parent_id) {
                relocated_span_ids.insert(s.id);
                s.parent_id = Some(relocate_to);
            }
        }
    }
}

#[instrument(skip_all, err(Debug))]
pub(crate) async fn collector_trace_data_post(
    State(app_state): State<AppState>,
    Json(trace_data): Json<api_structs::exporter::ExportedServiceTraceData>,
) -> Result<(), ApiError> {
    let con = app_state.con.clone();
    update_instance_data(&app_state, &trace_data);
    let service_id = trace_data.service_id;
    let service_name = trace_data.service_name;

    for mut fragment in trace_data.trace_fragments.into_values() {
        let db_trace = get_db_trace(&con, service_id, fragment.trace_id).await?;
        let trace_already_exists = db_trace.is_some();
        let trace_is_complete = db_trace
            .as_ref()
            .map(|t| t.duration.is_some())
            .unwrap_or(false);
        if trace_is_complete {
            error!("Got new data for completed trace");
            continue;
        }
        let mut relocated_span_ids: HashSet<u64> = HashSet::new();
        let mut relocated_event_vec_indexes: HashSet<usize> = HashSet::new();
        let roots: Vec<&NewSpan> = fragment
            .new_spans
            .iter()
            .filter(|e| e.parent_id.is_none())
            .collect();
        let root_duration = if trace_already_exists {
            match roots.len() {
                0 => {
                    trace!("trace already exists and we have no new root as expected");
                    None
                }
                _x => {
                    error!("Got new root for existing trace");
                    continue;
                }
            }
        } else {
            // trace doesnt exist yet
            match roots.len() {
                0 => {
                    warn!("Got fragment without root for non-existing trace, creating root");
                    if let Some(non_root_with_trace_id) = fragment
                        .new_spans
                        .iter()
                        .find(|e| e.id == fragment.trace_id)
                    {
                        error!(
                            "Got non-root span with same id as trace: {:?}",
                            non_root_with_trace_id
                        );
                        continue;
                    }
                    relocated_span_ids.insert(fragment.trace_id);
                    fragment.new_spans.push(NewSpan {
                        id: fragment.trace_id,
                        timestamp: fragment.trace_timestamp,
                        duration: None,
                        parent_id: None,
                        name: fragment.trace_name.clone(),
                        key_vals: Default::default(),
                    });
                    None
                }
                1 => {
                    trace!("Got root for new trace, as expected");
                    roots[0].duration
                }
                _x => {
                    error!("Got more than one root for new trace");
                    continue;
                }
            }
        };
        let mut known_and_unknown_span_ids = check_span_references(fragment.new_spans.as_slice());
        check_event_span_references(
            &mut known_and_unknown_span_ids,
            fragment.new_events.as_slice(),
        );
        let lost_span_ids = check_span_ids_exist_in_db_returning_missing(
            &con,
            &known_and_unknown_span_ids.unknown_span_ids,
            fragment.trace_id,
            service_id,
        )
        .await?;
        relocate_span_references_from_lost_spans_to_root(
            &mut fragment.new_spans,
            &lost_span_ids,
            &mut relocated_span_ids,
            fragment.trace_id,
        );
        relocate_event_references_from_lost_spans_to_root(
            &mut fragment.new_events,
            &lost_span_ids,
            &mut relocated_event_vec_indexes,
            fragment.trace_id,
        );
        let mut transaction = con.begin().await?;
        if !trace_already_exists {
            insert_new_trace(
                &mut transaction,
                service_id,
                fragment.trace_id,
                &service_name,
                &fragment.trace_name,
                fragment.trace_timestamp,
            )
            .await?;
        }
        insert_spans(
            &mut transaction,
            &fragment.new_spans,
            &lost_span_ids,
            fragment.trace_id,
            service_id,
            &relocated_span_ids,
        )
        .await?;
        insert_events(
            &mut transaction,
            &fragment.new_events,
            &lost_span_ids,
            fragment.trace_id,
            service_id,
            &relocated_event_vec_indexes,
        )
        .await?;
        let original_span_count = fragment.spe_count.span_count as u64;
        let original_event_count = fragment.spe_count.event_count as u64;
        let stored_span_count_increase = fragment.new_spans.len() as u64;
        let stored_event_count_increase = fragment.new_events.len() as u64;
        let event_char_count_increase = fragment.new_events.iter().fold(0u64, |mut acc, curr| {
            let size = curr.message.as_ref().map(|s| s.len()).unwrap_or(0);
            acc = acc.saturating_add(size as u64);
            acc
        });
        let warnings_count_increase = fragment
            .new_events
            .iter()
            .filter(|e| matches!(e.level, api_structs::exporter::Severity::Warn))
            .count() as u64;
        let has_errors = fragment
            .new_events
            .iter()
            .find(|e| matches!(e.level, api_structs::exporter::Severity::Error))
            .is_some();
        update_trace_header(
            &mut transaction,
            service_id,
            fragment.trace_id,
            root_duration,
            original_span_count,
            original_event_count,
            stored_span_count_increase,
            stored_event_count_increase,
            event_char_count_increase,
            warnings_count_increase,
            has_errors,
        )
        .await?;
        transaction.commit().await?;
    }

    Ok(())
}

async fn update_trace_header(
    con: &mut Transaction<'static, Postgres>,
    service_id: i64,
    trace_id: u64,
    duration: Option<u64>,
    original_span_count: u64,
    original_event_count: u64,
    stored_span_count_increase: u64,
    stored_event_count_increase: u64,
    event_char_count_increase: u64,
    warnings_count_increase: u64,
    has_errors: bool,
) -> Result<(), ApiError> {
    sqlx::query!(
        "update trace
        set duration=$3,
            original_span_count=$4,
            original_event_count=$5,
            stored_span_count=(stored_span_count + $6),
            stored_event_count=(stored_event_count + $7),
            event_char_count=(event_char_count + $8),
            warning_count=(warning_count + $9),
            has_errors=(has_errors or $10)
        where service_id = $1
          and id = $2;",
        service_id,
        trace_id as i64 as _,
        duration.map(|d| d as i64) as Option<i64>,
        original_span_count as i64 as _,
        original_event_count as i64 as _,
        stored_span_count_increase as i64 as _,
        stored_event_count_increase as i64 as _,
        event_char_count_increase as i64 as _,
        warnings_count_increase as i64 as _,
        has_errors,
    )
    .execute(con.deref_mut())
    .await?;
    Ok(())
}

pub(crate) async fn insert_events(
    con: &mut Transaction<'static, Postgres>,
    new_events: &[NewSpanEvent],
    lost_span_ids: &HashSet<u64>,
    trace_id: u64,
    service_id: i64,
    relocated_event_vec_indexes: &HashSet<usize>,
) -> Result<(), ApiError> {
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
    sqlx::query!(
            "insert into event (service_id, trace_id, span_id, timestamp, name, severity, relocated)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::TEXT[], $6::severity_level[], $7::BOOLEAN[]);",
            &service_ids,
            &trace_ids,
            &span_ids,
            &timestamps,
            &names as &Vec<Option<String>>,
            severities.as_slice() as &[Severity],
            &relocateds,
        )
        .execute(con.deref_mut())
        .await?;
    Ok(())
}

pub(crate) async fn insert_spans(
    con: &mut Transaction<'static, Postgres>,
    new_spans: &[NewSpan],
    lost_span_ids: &HashSet<u64>,
    trace_id: u64,
    service_id: i64,
    relocated_span_ids: &HashSet<u64>,
) -> Result<(), ApiError> {
    let span_ids: Vec<i64> = new_spans.iter().map(|s| s.id as i64).collect();
    let service_ids: Vec<i64> = new_spans.iter().map(|_s| service_id).collect();
    let trace_id: Vec<i64> = new_spans.iter().map(|_s| trace_id as i64).collect();
    let timestamp: Vec<i64> = new_spans.iter().map(|s| s.timestamp as i64).collect();
    let relocated: Vec<bool> = new_spans
        .iter()
        .map(|s| relocated_span_ids.contains(&s.id))
        .collect();
    let parent_id: Vec<Option<i64>> = new_spans
        .iter()
        .map(|s| s.parent_id.map(|e| e as i64))
        .collect();
    let duration: Vec<Option<i64>> = new_spans
        .iter()
        .map(|s| s.duration.map(|d| d as i64))
        .collect();
    let name: Vec<String> = new_spans.iter().map(|s| s.name.clone()).collect();
    sqlx::query!(
            "insert into span (id, service_id, trace_id, timestamp, parent_id, duration, name, relocated)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::BIGINT[], $6::BIGINT[], $7::TEXT[], $8::BOOLEAN[]);",
            &span_ids,
            &service_ids,
            &trace_id,
            &timestamp,
            &parent_id as &Vec<Option<i64>>,
            &duration as &Vec<Option<i64>>,
            &name,
            &relocated,
        )
        .execute(con.deref_mut())
        .await?;
    Ok(())
}

// pub(crate) async fn insert_new_trace(
//     con: &PgPool,
//     service_name: &str,
//     service_id: i64,
//     new_span: &api_structs::exporter::NewSpan,
// ) -> Result<(), ApiError> {
//     let timestamp = i64::try_from(new_span.timestamp).expect("timestamp to fit i64");
//     let trace_id = i64::try_from(new_span.trace_id.get()).expect("id to fit i64");
//     sqlx::query!(
//         "insert into trace (service_id, id, timestamp, service_name, \
//         top_level_span_name, duration, warning_count, has_errors) values \
//         ($1, $2, $3, $4, $5, null, 0, false);",
//         service_id,
//         trace_id as _,
//         timestamp as _,
//         service_name as _,
//         new_span.name as _
//     )
//     .execute(con)
//     .await?;
//     Ok(())
// }

// pub(crate) async fn insert_new_span(
//     con: &PgPool,
//     service_id: i64,
//     new_span: &api_structs::exporter::NewSpan,
// ) -> Result<(), ApiError> {
//     let timestamp = i64::try_from(new_span.timestamp).expect("timestamp to fit i64");
//     let trace_id = i64::try_from(new_span.trace_id.get()).expect("id to fit i64");
//     let span_id = i64::try_from(new_span.id).expect("id to fit i64");
//     let parent_id = new_span
//         .parent_id
//         .map(|e| i64::try_from(e).expect("id to fit i64"));
//     sqlx::query!(
//         "insert into span (id, service_id, trace_id, timestamp, parent_id, \
//         duration, name) values \
//         ($1, $2, $3, $4, $5, null, $6);",
//         span_id as _,
//         service_id,
//         trace_id as _,
//         timestamp as _,
//         parent_id as _,
//         new_span.name as _
//     )
//     .execute(con)
//     .await?;
//     Ok(())
// }

// pub(crate) async fn process_new_span_event(
//     con: &PgPool,
//     service_id: i64,
//     span_event: api_structs::exporter::NewSpanEvent,
// ) -> Result<(), ApiError> {
//     let timestamp = i64::try_from(span_event.timestamp).expect("timestamp to fit i64");
//     let span_id = i64::try_from(span_event.span_id).expect("id to fit i64");
//     let trace_id = i64::try_from(span_event.trace_id.get()).expect("id to fit i64");
//     let id = i64::try_from(span_event.id).expect("id to fit i64");
//     let level = Severity::from(span_event.level);
//
//     sqlx::query!(
//         "insert into event (service_id, trace_id, span_id, id, timestamp, name, \
//         severity) values \
//         ($1, $2, $3, $4, $5, $6, $7);",
//         service_id,
//         trace_id,
//         span_id,
//         id,
//         timestamp as _,
//         span_event.message as _,
//         level as Severity
//     )
//     .execute(con)
//     .await?;
//     Ok(())
// }

pub(crate) async fn process_closed_span(
    con: &PgPool,
    service_id: i64,
    closed_span: api_structs::exporter::ClosedSpan,
) -> Result<(), ApiError> {
    let id = i64::try_from(closed_span.span_id).expect("id to fit i64");
    let duration = i64::try_from(closed_span.duration).expect("duration to fit i64");
    let parent_id: Option<i64> = sqlx::query_scalar!(
        "update span set duration=$1 where id=$2 and service_id=$3 returning parent_id",
        duration as _,
        id,
        service_id
    )
    .fetch_one(con)
    .await?;
    if parent_id.is_none() {
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
// pub(crate) async fn process_new_span(
//     con: &PgPool,
//     service_name: &str,
//     service_id: i64,
//     new_span: api_structs::exporter::NewSpan,
// ) -> Result<(), ApiError> {
//     if new_span.parent_id.is_none() {
//         insert_new_trace(con, service_name, service_id, &new_span).await?;
//         insert_new_span(con, service_id, &new_span).await?;
//     } else {
//         insert_new_span(con, service_id, &new_span).await?;
//     }
//     Ok(())
// }

pub(crate) async fn insert_orphan_events(
    con: &PgPool,
    service_name: &str,
    orphan_events: Vec<NewOrphanEvent>,
) -> Result<(), ApiError> {
    for event in orphan_events {
        let timestamp = nanos_to_db_i64(event.timestamp)?;
        let level = Severity::from(event.level);
        sqlx::query!(
        "insert into log (timestamp, service_name, severity, value) VALUES ($1::ubigint, $2, $3, $4);",
        timestamp as _,
        service_name as _,
        &level as &Severity,
        event.message as _
    ).execute(con).await?;
    }
    Ok(())
}
