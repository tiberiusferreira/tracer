use crate::api::{ApiError, AppState, ChangeFilterInternalRequest, LiveInstances, ServiceName};
use crate::{SINGLE_KEY_VALUE_KEY_CHARS_LIMIT, SINGLE_KEY_VALUE_VALUE_CHARS_LIMIT};
use api_structs::exporter::trace_exporting::{
    ClosedSpan, ExportedServiceTraceData, NewOrphanEvent, NewSpan, NewSpanEvent, TraceFragment,
};
use api_structs::time_conversion::now_nanos_u64;
use api_structs::ui::live_services::LiveServiceInstance;
use api_structs::ui::orphan_events::{OrphanEvent, ServiceOrphanEventsRequest};
use axum::extract::{Query, State};
use axum::Json;
use reqwest::StatusCode;
use sqlx::postgres::PgQueryResult;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::ops::{Deref, DerefMut};
use tracing::{debug, error, info, info_span, instrument, trace, Instrument};

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
    pub fn to_api(&self) -> api_structs::Severity {
        match self {
            Severity::Trace => api_structs::Severity::Trace,
            Severity::Debug => api_structs::Severity::Debug,
            Severity::Info => api_structs::Severity::Info,
            Severity::Warn => api_structs::Severity::Warn,
            Severity::Error => api_structs::Severity::Error,
        }
    }
}
impl From<api_structs::Severity> for Severity {
    fn from(value: api_structs::Severity) -> Self {
        match value {
            api_structs::Severity::Trace => Self::Trace,
            api_structs::Severity::Debug => Self::Debug,
            api_structs::Severity::Info => Self::Info,
            api_structs::Severity::Warn => Self::Warn,
            api_structs::Severity::Error => Self::Error,
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

#[instrument(level = "error", skip_all, err(Debug))]
pub(crate) async fn instances_filter_post(
    State(app_state): State<AppState>,
    Json(new_filter): Json<api_structs::ui::NewFiltersRequest>,
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

#[instrument(level = "error", skip_all)]
pub(crate) async fn logs_get(
    service_log_request: Query<ServiceOrphanEventsRequest>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<OrphanEvent>>, ApiError> {
    let from_timestamp = nanos_to_db_i64(service_log_request.from_date_unix)?;
    let to_timestamp = nanos_to_db_i64(service_log_request.to_date_unix)?;
    let service_name = &service_log_request.service_name;
    pub struct DbLog {
        pub timestamp: i64,
        pub severity: Severity,
        pub message: Option<String>,
        pub key_vals: sqlx::types::JsonValue,
    }
    let service_list: Vec<DbLog> = sqlx::query_as!(
        DbLog,
        r#"select orphan_event.timestamp,
       severity       as "severity: Severity",
       orphan_event.message,
       COALESCE(json_object_agg(
                                  orphan_event_key_value.key,
                                  orphan_event_key_value.value
                         )
                filter ( where orphan_event_key_value.key is not null),
                '{}') as key_vals
from orphan_event
         left join orphan_event_key_value
                   on orphan_event_key_value.orphan_event_id = orphan_event.id
where orphan_event.timestamp >= $1 and orphan_event.timestamp <= $2 and orphan_event.service_name=$3
group by orphan_event.id, orphan_event.timestamp
order by timestamp desc
limit 100000;"#,
        from_timestamp,
        to_timestamp,
        service_name
    )
    .fetch_all(&app_state.con)
    .await?;

    Ok(Json(
        service_list
            .into_iter()
            .map(|e| OrphanEvent {
                timestamp: db_i64_to_nanos(e.timestamp).expect("db timestamp to fit u64"),
                severity: e.severity.to_api(),
                message: e.message,
                key_vals: serde_json::from_value(e.key_vals)
                    .expect("to be able to deserialize event kv from DB"),
            })
            .collect(),
    ))
}
#[instrument(level = "error", skip_all)]
pub(crate) async fn orphan_events_service_names_get(
    State(app_state): State<AppState>,
) -> Result<Json<Vec<ServiceName>>, ApiError> {
    let service_list: Vec<String> =
        sqlx::query_scalar!("select distinct orphan_event.service_name from orphan_event;")
            .fetch_all(&app_state.con)
            .await?;
    debug!("Got {} services", service_list.len());
    trace!("Got services: {:#?}", service_list);
    Ok(Json(service_list))
}

#[instrument(level = "error", skip_all)]
pub(crate) async fn instances_get(
    State(app_state): State<AppState>,
) -> Result<Json<api_structs::ui::live_services::LiveInstances>, ApiError> {
    let instances: HashMap<ServiceName, Vec<LiveServiceInstance>> =
        app_state.live_instances.trace_data.read().deref().clone();
    Ok(Json(api_structs::ui::live_services::LiveInstances {
        instances,
    }))
}

#[instrument(skip_all)]
fn update_instance_data(live_instances: &LiveInstances, service_data: &ExportedServiceTraceData) {
    let service_id = service_data.service_id;
    let service_name = &service_data.service_name;
    debug!("locking instances trace_data to update instance {service_id} of {service_name}");
    let mut instances = live_instances.trace_data.write();
    debug!("locked");
    let entry = instances
        .entry(service_data.service_name.to_string())
        .or_default();
    let new = LiveServiceInstance {
        last_seen_timestamp: now_nanos_u64(),
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

#[instrument(skip_all)]
async fn get_db_trace(
    con: &PgPool,
    service_id: i64,
    trace_id: u64,
) -> Result<Option<TraceHeader>, sqlx::Error> {
    debug!("service_id: {service_id}, trace_id: {trace_id}");
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

#[instrument(skip_all)]
fn check_event_span_references_for_sorted(
    known_and_unknown_ids: &mut KnownAndUnknownIds,
    events: &[NewSpanEvent],
) {
    for e in events {
        let span_id = e.span_id;
        if !known_and_unknown_ids.known_span_ids.contains(&span_id) {
            let event_timestamp = e.timestamp;
            debug!("Event timestamp {event_timestamp} belongs to span {span_id} outside trace fragment");
            known_and_unknown_ids.unknown_span_ids.insert(span_id);
        }
    }
}

#[instrument(skip_all)]
fn check_span_references_for_sorted(span: &[NewSpan]) -> KnownAndUnknownIds {
    let mut known_span_ids = HashSet::new();
    let mut unknown_span_ids = HashSet::new();
    for s in span {
        if let Some(parent_id) = s.parent_id {
            if !known_span_ids.contains(&parent_id) {
                let span_id = s.id;
                debug!("Span {span_id} has parent {parent_id} outside trace fragment");
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

#[instrument(skip_all)]
async fn check_span_ids_exist_in_db_returning_missing(
    con: &PgPool,
    span_ids_to_check: &HashSet<u64>,
    trace_id: u64,
    service_id: i64,
) -> Result<HashSet<u64>, sqlx::Error> {
    if span_ids_to_check.is_empty() {
        debug!("Span ids to check is empty, returning empty list");
        return Ok(HashSet::new());
    }
    let as_vec: Vec<i64> = span_ids_to_check.iter().map(|e| *e as i64).collect();
    debug!("Getting {} span ids from the db", as_vec.len());
    trace!("Span ids: {:?}", as_vec);
    let res: Vec<i64> = sqlx::query_scalar!(
        "select id from span where trace_id=$1 and service_id=$2 and id = ANY($3::BIGINT[])",
        trace_id as i64,
        service_id,
        as_vec.as_slice()
    )
    .fetch_all(con)
    .await?;
    debug!("Got {} back", res.len());
    trace!("Span ids from DB {:?}", res);
    let existing_ids: HashSet<u64> = res.iter().map(|id| *id as u64).collect();
    let missing_ids: HashSet<u64> = span_ids_to_check
        .difference(&existing_ids)
        .cloned()
        .collect();
    Ok(missing_ids)
}

#[instrument(skip_all)]
async fn insert_new_trace(
    con: &mut Transaction<'static, Postgres>,
    service_id: i64,
    trace_id: u64,
    service_name: &str,
    top_level_span_name: &str,
    timestamp: u64,
) -> Result<(), sqlx::Error> {
    info!(
        "Inserting trace header information for: {}",
        top_level_span_name
    );
    let now_nanos = now_nanos_u64();
    if let Err(e) = sqlx::query!(
        "insert into trace (service_id, id, service_name, timestamp, top_level_span_name,
                    updated_at) values (
                    $1, $2, $3, $4, $5, $6);",
        service_id as _,
        trace_id as i64,
        service_name as _,
        timestamp as i64,
        top_level_span_name as _,
        now_nanos as i64
    )
    .execute(con.deref_mut())
    .await
    {
        error!(
            "DB Error when inserting trace with data:\
         service_id={service_id} trace_id={trace_id} service_name={service_name} \
         timestamp={timestamp} top_level_span_name={top_level_span_name} updated_at={now_nanos}"
        );
        return Err(e);
    }
    Ok(())
}

#[instrument(skip_all)]
fn relocate_event_references_from_lost_spans_to_root(
    events: &mut Vec<NewSpanEvent>,
    lost_span_ids: &HashSet<u64>,
    relocated_event_vec_indexes: &mut HashSet<usize>,
    relocate_to: u64,
) {
    for (idx, e) in events.iter_mut().enumerate() {
        if lost_span_ids.contains(&e.span_id) {
            debug!(
                "Relocating event {} span from {} to {}",
                e.timestamp, e.span_id, relocate_to
            );
            relocated_event_vec_indexes.insert(idx);
            e.span_id = relocate_to;
        }
    }
}

#[instrument(skip_all)]
fn relocate_span_references_from_lost_spans_to_root(
    spans: &mut Vec<NewSpan>,
    lost_span_ids: &HashSet<u64>,
    relocated_span_ids: &mut HashSet<u64>,
    relocate_to: u64,
) {
    for s in spans {
        if let Some(parent_id) = s.parent_id {
            if lost_span_ids.contains(&parent_id) {
                debug!(
                    "Relocating span {} parent from {} to {}",
                    s.id, parent_id, relocate_to
                );
                relocated_span_ids.insert(s.id);
                s.parent_id = Some(relocate_to);
            }
        }
    }
}

#[instrument(skip_all)]
pub async fn update_trace_with_new_fragment(
    con: &PgPool,
    service_id: i64,
    service_name: &str,
    mut fragment: TraceFragment,
) -> Result<(), sqlx::Error> {
    info!("fragment for: {}", fragment.trace_name);
    fragment.new_events.sort_by_key(|e| e.timestamp);
    fragment.new_spans.sort_by_key(|e| e.timestamp);
    let db_trace = get_db_trace(&con, service_id, fragment.trace_id).await?;
    let trace_already_exists = db_trace.is_some();
    debug!("trace_already_exists = {trace_already_exists}");
    let trace_is_complete = db_trace
        .as_ref()
        .map(|t| t.duration.is_some())
        .unwrap_or(false);
    debug!("trace_is_complete = {trace_is_complete}");
    if trace_is_complete {
        error!("Got new data for completed trace");
        return Ok(());
    }
    let mut relocated_span_ids: HashSet<u64> = HashSet::new();
    let mut relocated_event_vec_indexes: HashSet<usize> = HashSet::new();
    let roots: Vec<&NewSpan> = fragment
        .new_spans
        .iter()
        .filter(|e| e.parent_id.is_none())
        .collect();
    debug!("root count = {}", roots.len());
    trace!("roots = {:#?}", roots);
    let root_duration = if trace_already_exists {
        match roots.len() {
            0 => {
                debug!("trace already exists and we have no new root as expected");
                None
            }
            _x => {
                error!("Got new root for existing trace");
                return Ok(());
            }
        }
    } else {
        // trace doesnt exist yet
        match roots.len() {
            0 => {
                debug!("Got fragment without root for non-existing trace, creating root with id=trace_id: {}", fragment.trace_id);
                if let Some(non_root_with_trace_id) = fragment
                    .new_spans
                    .iter()
                    .find(|e| e.id == fragment.trace_id)
                {
                    error!(
                        "Got non-root span with same id as trace: {:?}",
                        non_root_with_trace_id
                    );
                    return Ok(());
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
                debug!("Got root for new trace, as expected");
                roots[0].duration
            }
            _x => {
                error!("Got more than one root for new trace");
                return Ok(());
            }
        }
    };
    let mut known_and_unknown_span_ids =
        check_span_references_for_sorted(fragment.new_spans.as_slice());
    trace!("{:#?}", known_and_unknown_span_ids);
    check_event_span_references_for_sorted(
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
    debug!("{} lost span ids", lost_span_ids.len());
    trace!("Lost span ids: {:?}", lost_span_ids);
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
    debug!("Trying to start transaction");
    let mut transaction = con
        .begin()
        .instrument(info_span!("start_transaction"))
        .await?;
    debug!("Started!");
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
        fragment.trace_id,
        service_id,
        &relocated_span_ids,
    )
    .await?;
    crate::api::database::insert_events(
        &mut transaction,
        &fragment.new_events,
        fragment.trace_id,
        service_id,
        &relocated_event_vec_indexes,
    )
    .await?;
    let original_span_count = fragment.spe_count.span_count as u64;
    let original_event_count = fragment.spe_count.event_count as u64;
    let stored_span_count_increase = fragment.new_spans.len() as u64;
    let stored_event_count_increase = fragment.new_events.len() as u64;
    let event_bytes_count_increase = fragment.new_events.iter().fold(0u64, |mut acc, curr| {
        let size = curr.message.as_ref().map(|s| s.len()).unwrap_or(0);
        acc = acc.saturating_add(size as u64);
        acc
    });
    let warnings_count_increase = fragment
        .new_events
        .iter()
        .filter(|e| matches!(e.level, api_structs::Severity::Warn))
        .count() as u64;
    let has_errors = fragment
        .new_events
        .iter()
        .find(|e| matches!(e.level, api_structs::Severity::Error))
        .is_some();
    debug!(
        "root_duration={:?}
original_span_count={original_span_count}
original_event_count={original_event_count}
stored_span_count_increase={stored_span_count_increase}
stored_event_count_increase={stored_event_count_increase}
event_bytes_count_increase={event_bytes_count_increase}
warnings_count_increase={warnings_count_increase}
has_errors={has_errors}",
        root_duration
    );
    update_trace_header(
        &mut transaction,
        service_id,
        fragment.trace_id,
        root_duration,
        original_span_count,
        original_event_count,
        stored_span_count_increase,
        stored_event_count_increase,
        event_bytes_count_increase,
        warnings_count_increase,
        has_errors,
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

#[instrument(skip_all)]
async fn update_closed_spans(con: &PgPool, service_id: i64, closed_spans: &[ClosedSpan]) {
    info!("{} spans to close", closed_spans.len());
    for span in closed_spans {
        debug!("Closing span: {:?}", span);
        let res: Result<PgQueryResult, sqlx::Error> = sqlx::query!(
            "update span set duration=$1 where service_id=$2 and trace_id=$3 and id=$4;",
            span.duration as i64,
            service_id,
            span.trace_id as i64,
            span.span_id as i64,
        )
        .execute(con)
        .await;
        match res {
            Ok(res) => {
                debug!("Updated ({} rows)", res.rows_affected());
            }
            Err(err) => {
                error!("Error closing span {err:?} {span:?}");
            }
        }
        if span.span_id == span.trace_id {
            info!("Span was root, updating trace duration");
            let res = sqlx::query!(
                "update trace set duration=$1 where service_id=$2 and id=$3",
                span.duration as i64,
                service_id,
                span.trace_id as i64
            )
            .execute(con)
            .await;
            match res {
                Ok(res) => {
                    debug!("Updated ({} rows)", res.rows_affected());
                }
                Err(err) => {
                    error!(
                        "Error updating trace {err:?} duration={} service_id={} id={}",
                        span.duration, service_id, span.trace_id
                    );
                }
            }
        }
    }
}

#[instrument(skip_all)]
pub async fn insert_orphan_events(
    con: &PgPool,
    service_name: &str,
    orphan_events: &[NewOrphanEvent],
) {
    info!("{} events to insert", orphan_events.len());
    for e in orphan_events {
        trace!("Inserting event: {:?}", e);
    }
    let mut timestamps = vec![];
    let mut service_names = vec![];
    let mut severities = vec![];
    let mut message = vec![];
    for event in orphan_events {
        timestamps.push(event.timestamp as i64);
        service_names.push(service_name);
        severities.push(Severity::from(event.level));
        message.push(event.message.clone());
    }
    let orphan_events_db_ids = match sqlx::query_scalar!(
            "insert into orphan_event (timestamp, service_name, severity, message) select * from unnest($1::BIGINT[], \
            $2::TEXT[], $3::severity_level[], $4::TEXT[]) returning id;",
            &timestamps,
            &service_names as &Vec<&str>,
            severities.as_slice() as &[Severity],
            &message as &Vec<Option<String>>
        )
        .fetch_all(con).await{
        Ok(res) => {
            debug!("Inserted and got {} ids back", res.len());
            let res: Vec<i64> = res;
            res
        },
        Err(e) => {
            error!("Error inserting orphan events: {:#?}", e);
            error!("timestamp={:?}", timestamps);
            error!("service_names={:?}", service_names);
            error!("severities={:?}", severities);
            for v in message {
                error!("message={:?}", v);
            }
            return;
        }
    };
    let mut kv_orphan_event_id = vec![];
    let mut kv_orphan_timestamp = vec![];
    let mut kv_orphan_key = vec![];
    let mut kv_orphan_value = vec![];
    for (idx, event) in orphan_events.iter().enumerate() {
        for (key, val) in &event.key_vals {
            kv_orphan_event_id.push(orphan_events_db_ids[idx]);
            kv_orphan_timestamp.push(event.timestamp as i64);
            kv_orphan_key.push(key.as_str());
            kv_orphan_value.push(val.as_str());
        }
    }
    info!("{} events key values to insert", kv_orphan_event_id.len());

    match sqlx::query!(
            "insert into orphan_event_key_value (orphan_event_id, timestamp, key, value) select * from unnest($1::BIGINT[], \
            $2::BIGINT[], $3::TEXT[], $4::TEXT[]);",
            &kv_orphan_event_id,
            &kv_orphan_timestamp,
            &kv_orphan_key as &Vec<&str>,
            &kv_orphan_value as &Vec<&str>,
        )
        .execute(con).await{
        Ok(res) => {
            debug!("Inserted {}", res.rows_affected());
        },
        Err(e) => {
            error!("Error inserting orphan events: {:#?}", e);
            error!("kv_orphan_event_id={:?}", kv_orphan_event_id);
            error!("kv_orphan_timestamp={:?}", kv_orphan_timestamp);
            error!("kv_orphan_key={:?}", kv_orphan_key);
            error!("kv_orphan_value={:?}", kv_orphan_value);
        }
    };
}

fn truncate_string(string: &str, max_chars: usize) -> String {
    string.chars().take(max_chars).collect::<String>()
}

fn truncate_key_values(key_vals: &mut HashMap<String, String>) {
    let keys_too_big: HashMap<String, String> = key_vals
        .keys()
        .filter_map(|k| {
            if k.len() > SINGLE_KEY_VALUE_KEY_CHARS_LIMIT {
                let new_key = truncate_string(&k, SINGLE_KEY_VALUE_KEY_CHARS_LIMIT);
                info!(
                    "Truncating key (too big), starts with: {}",
                    truncate_string(&new_key, 100)
                );
                Some((k.clone(), new_key))
            } else {
                None
            }
        })
        .collect();
    for (key, replacement_key) in keys_too_big {
        let val = key_vals.remove(&key).unwrap();
        key_vals.insert(replacement_key, val);
    }
    for (key, val) in key_vals {
        if val.len() > SINGLE_KEY_VALUE_VALUE_CHARS_LIMIT {
            *val = truncate_string(val, SINGLE_KEY_VALUE_VALUE_CHARS_LIMIT);
            info!(
                "Truncating value (too big) for key {key} value: {}",
                truncate_string(val, 100)
            );
        }
    }
}

#[instrument(skip_all)]
fn truncate_span_key_values_if_needed(spans: &mut Vec<NewSpan>) {
    for s in spans {
        truncate_key_values(&mut s.key_vals);
    }
}

#[instrument(skip_all)]
fn truncate_events_if_needed(events: &mut Vec<NewSpanEvent>) {
    for e in events {
        if let Some(msg) = &mut e.message {
            if msg.len() > crate::SINGLE_EVENT_CHARS_LIMIT {
                info!("Truncating event (too big): {}", truncate_string(&msg, 100));
                *msg = truncate_string(msg, crate::SINGLE_EVENT_CHARS_LIMIT);
            }
        }
        truncate_key_values(&mut e.key_vals);
    }
}

#[instrument(skip_all)]
fn truncate_orphan_events_if_needed(events: &mut Vec<NewOrphanEvent>) {
    for e in events {
        if let Some(msg) = &mut e.message {
            if msg.len() > crate::SINGLE_EVENT_CHARS_LIMIT {
                info!(
                    "Truncating orphan event (too big): {}",
                    truncate_string(&msg, 100)
                );
                *msg = truncate_string(msg, crate::SINGLE_EVENT_CHARS_LIMIT);
            }
        }
        truncate_key_values(&mut e.key_vals);
    }
}

#[instrument(level = "error", skip_all, err(Debug))]
pub async fn collector_trace_data_post(
    State(app_state): State<AppState>,
    compressed_json: axum::body::Bytes,
) -> Result<(), ApiError> {
    let compressed_json = compressed_json.to_vec();
    let mut reader = brotli::Decompressor::new(
        compressed_json.as_slice(),
        16384, // buffer size
    );
    let mut json = String::new();
    reader.read_to_string(&mut json).map_err(|e| {
        error!("Error decompressing request body: {e:#?}");
        ApiError {
            code: StatusCode::BAD_REQUEST,
            message: "Error reading compressed request body".to_string(),
        }
    })?;
    let trace_data: ExportedServiceTraceData =
        serde_json::from_str(json.as_str()).map_err(|e| {
            error!(
                "Invalid Json in request body: {}\n{e:#?}",
                json.chars().take(5000).collect::<String>()
            );
            ApiError {
                code: StatusCode::BAD_REQUEST,
                message: "Decoded into string, but json was invalid".to_string(),
            }
        })?;
    let con = app_state.con;
    update_instance_data(&app_state.live_instances, &trace_data);
    let service_id = trace_data.service_id;
    let service_name = trace_data.service_name;
    info!("Got {} new fragments", trace_data.trace_fragments.len());
    for mut fragment in trace_data.trace_fragments.into_values() {
        truncate_span_key_values_if_needed(&mut fragment.new_spans);
        truncate_events_if_needed(&mut fragment.new_events);
        if let Err(db_error) =
            update_trace_with_new_fragment(&con, service_id, &service_name, fragment).await
        {
            error!("DB error when inserting fragment: {:#?}", db_error);
        }
    }

    update_closed_spans(&con, service_id, &trace_data.closed_spans).await;
    let mut orphan_events = trace_data.orphan_events;
    truncate_orphan_events_if_needed(&mut orphan_events);
    insert_orphan_events(&con, &service_name, &orphan_events).await;

    Ok(())
}

#[instrument(skip_all)]
async fn update_trace_header(
    con: &mut Transaction<'static, Postgres>,
    service_id: i64,
    trace_id: u64,
    duration: Option<u64>,
    original_span_count: u64,
    original_event_count: u64,
    stored_span_count_increase: u64,
    stored_event_count_increase: u64,
    event_bytes_count_increase: u64,
    warnings_count_increase: u64,
    has_errors: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update trace
        set duration=$3,
            original_span_count=$4,
            original_event_count=$5,
            stored_span_count=(stored_span_count + $6),
            stored_event_count=(stored_event_count + $7),
            event_bytes_count=(event_bytes_count + $8),
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
        event_bytes_count_increase as i64 as _,
        warnings_count_increase as i64 as _,
        has_errors,
    )
    .execute(con.deref_mut())
    .await?;
    Ok(())
}

#[instrument(skip_all)]
pub(crate) async fn insert_spans(
    con: &mut Transaction<'static, Postgres>,
    new_spans: &[NewSpan],
    trace_id: u64,
    service_id: i64,
    relocated_span_ids: &HashSet<u64>,
) -> Result<(), sqlx::Error> {
    if new_spans.is_empty() {
        info!("No spans to insert");
        return Ok(());
    } else {
        info!("Inserting {} spans", new_spans.len());
    }
    let span_ids: Vec<i64> = new_spans.iter().map(|s| s.id as i64).collect();
    let service_ids: Vec<i64> = new_spans.iter().map(|_s| service_id).collect();
    let trace_ids: Vec<i64> = new_spans.iter().map(|_s| trace_id as i64).collect();
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
    match sqlx::query!(
            "insert into span (id, service_id, trace_id, timestamp, parent_id, duration, name, relocated)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::BIGINT[], $6::BIGINT[], $7::TEXT[], $8::BOOLEAN[]);",
            &span_ids,
            &service_ids,
            &trace_ids,
            &timestamp,
            &parent_id as &Vec<Option<i64>>,
            &duration as &Vec<Option<i64>>,
            &name,
            &relocated,
        )
        .execute(con.deref_mut())
        .await {
        Ok(_) => {
            info!("Inserted spans");
        },
        Err(e) => {
            error!("Error when inserting spans");
            error!("Span Ids: {:?}", span_ids);
            error!("Service Ids: {:?}", service_ids);
            error!("Trace Ids: {:?}", trace_id);
            error!("Timestamp: {:?}", timestamp);
            error!("Relocated: {:?}", relocated);
            error!("parent_id: {:?}", parent_id);
            error!("duration: {:?}", duration);
            error!("name: {:?}", name);
            return Err(e)
        }
    };
    let mut kv_service_id = vec![];
    let mut kv_trace_id = vec![];
    let mut kv_span_id = vec![];
    let mut kv_timestamp = vec![];
    let mut kv_key = vec![];
    let mut kv_value = vec![];
    for (_idx, span) in new_spans.iter().enumerate() {
        for (key, val) in &span.key_vals {
            kv_service_id.push(service_id);
            kv_trace_id.push(trace_id as i64);
            kv_span_id.push(span.id as i64);
            kv_timestamp.push(span.timestamp as i64);
            kv_key.push(key.as_str());
            kv_value.push(val.as_str());
        }
    }
    if kv_service_id.is_empty() {
        info!("No span key-values to insert");
        return Ok(());
    } else {
        info!("Inserting {} span key-values", kv_service_id.len());
    }
    match sqlx::query!(
            "insert into span_key_value (service_id, trace_id, span_id, timestamp, key, value)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::TEXT[], $6::TEXT[]);",
            &kv_service_id,
            &kv_trace_id,
            &kv_span_id,
            &kv_timestamp,
            &kv_key as &Vec<&str>,
            &kv_value as &Vec<&str>
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
