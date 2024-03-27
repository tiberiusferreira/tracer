use std::collections::hash_map::ValuesMut;
use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use sqlx::{PgPool, Postgres, Transaction};
use tracing::{debug, error, info, info_span, instrument, trace, Instrument};

use api_structs::instance::update::{
    ClosedSpan, ExportedServiceTraceData, NewOrphanEvent, NewSpan, NewSpanEvent, OpenSpan,
    Sampling, SamplingState, TraceState,
};
use api_structs::time_conversion::now_nanos_u64;
use api_structs::ui::service::{OrphanEvent, ProfileData, TraceHeader};
use api_structs::{InstanceId, ServiceId, TraceName};
use backtraced_error::SqlxError;

use crate::api::handlers::Severity;
use crate::api::state::{AppState, BytesBudgetUsage, ServiceDataPoint, Shared};
use crate::api::ApiError;
use crate::{
    MAX_STATS_HISTORY_DATA_COUNT, SINGLE_KEY_VALUE_KEY_CHARS_LIMIT,
    SINGLE_KEY_VALUE_VALUE_CHARS_LIMIT,
};

pub struct ServiceNotRegisteredError;

#[instrument(skip_all)]
fn update_service_and_instance_data(
    live_instances: &Shared<HashMap<ServiceId, crate::api::state::ServiceRuntimeData>>,
    exported_service_trace_data: &ExportedServiceTraceData,
) -> Result<Sampling, ServiceNotRegisteredError> {
    let mut w_lock = live_instances.write();
    let service_data = match w_lock.get_mut(&exported_service_trace_data.instance_id.service_id) {
        None => return Err(ServiceNotRegisteredError),
        Some(service_data) => service_data,
    };
    let instance = match service_data
        .instances
        .get_mut(&exported_service_trace_data.instance_id.instance_id)
    {
        None => return Err(ServiceNotRegisteredError),
        Some(instance) => instance,
    };
    {
        instance.rust_log = exported_service_trace_data.rust_log.clone();
        instance.last_seen = std::time::Instant::now();
        if let Some(profile_data) = &exported_service_trace_data.profile_data {
            instance.profile_data = Some(ProfileData {
                profile_data_timestamp: now_nanos_u64(),
                profile_data: profile_data.clone(),
            });
        }
    }

    let mut traces_header = vec![];
    let mut received_bytes_per_trace: HashMap<TraceName, u64> = HashMap::new();
    let received_orphan_event_bytes = exported_service_trace_data.orphan_events_size();
    for trace_state in exported_service_trace_data.traces_state.values() {
        let received_trace_bytes = trace_state.total_size();
        let entry = received_bytes_per_trace
            .entry(trace_state.root_span.name.clone())
            .or_default();
        *entry += received_trace_bytes as u64;
        let header = TraceHeader {
            trace_id: trace_state.root_span.id,
            trace_name: trace_state.root_span.name.clone(),
            trace_timestamp: trace_state.root_span.timestamp,
            new_warnings: trace_state.has_warnings(),
            new_errors: trace_state.has_errors(),
            fragment_bytes: received_trace_bytes as u64,
            duration: trace_state.root_span.duration,
        };
        traces_header.push(header);
    }
    let mut last_bytes_budget = service_data
        .service_data_points
        .back()
        .map(|b| b.budget_usage.clone())
        .unwrap_or_else(|| BytesBudgetUsage::new(60, 100_000));
    debug!("Previous budget: {last_bytes_budget:?}");
    last_bytes_budget.update();
    debug!("Previous budget after update: {last_bytes_budget:?}");

    debug!("Decreasing orphan events budget by {received_orphan_event_bytes}");
    last_bytes_budget.increase_orphan_events_usage_by(received_orphan_event_bytes as u32);
    for (trace_name, received_bytes) in &received_bytes_per_trace {
        debug!("Decreasing {trace_name} budget by {received_bytes}");
        last_bytes_budget.increase_trace_usage_by(trace_name, *received_bytes as u32);
    }
    let remaining_budget = last_bytes_budget;
    debug!("remaining_budget = {remaining_budget:?}");
    let new_sampling = Sampling {
        traces: remaining_budget
            .traces_usage_bytes
            .iter()
            .map(|(name, _usage)| {
                let sampling = if remaining_budget.is_trace_over_budget(name) {
                    SamplingState::DropNewTracesKeepExistingTraceNewData
                } else {
                    SamplingState::AllowNewTraces
                };
                (name.to_string(), sampling)
            })
            .collect::<HashMap<TraceName, SamplingState>>(),
        allow_new_orphan_events: !remaining_budget.is_orphan_events_over_budget(),
    };
    debug!("new_sampling = {new_sampling:?}");
    let new = ServiceDataPoint {
        timestamp: now_nanos_u64(),
        instance_id: exported_service_trace_data.instance_id.instance_id,
        traces: traces_header,
        orphan_events: exported_service_trace_data
            .orphan_events
            .iter()
            .map(|e| OrphanEvent {
                timestamp: e.timestamp,
                severity: e.severity,
                message: e.message.clone(),
                key_vals: e.key_vals.clone(),
            })
            .collect(),
        budget_usage: remaining_budget,
    };
    service_data.service_data_points.push_back(new);
    while service_data.service_data_points.len() > MAX_STATS_HISTORY_DATA_COUNT {
        service_data.service_data_points.pop_front();
    }
    trace!("{:?}", new_sampling);
    Ok(new_sampling)
}

/// We should insert a new trace if it doesn't already exist

struct RawTraceHeader {
    duration: Option<i64>,
}

pub struct TraceDuration {
    pub duration: Option<u64>,
}

#[instrument(skip_all)]
async fn get_db_trace(
    con: &PgPool,
    instance_id: &InstanceId,
    trace_id: u64,
) -> Result<Option<TraceDuration>, backtraced_error::SqlxError> {
    debug!("instance_id: {instance_id:?}, trace_id: {trace_id}");
    let raw: Option<RawTraceHeader> = sqlx::query_as!(
        RawTraceHeader,
        "select duration from trace where instance_id=$1 and id=$2",
        instance_id.instance_id as i64,
        trace_id as i64
    )
    .fetch_optional(con)
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, "get_db_trace"))?;
    return match raw {
        None => Ok(None),
        Some(raw) => Ok(Some(TraceDuration {
            duration: raw.duration.map(|e| e as u64),
        })),
    };
}

#[instrument(skip_all)]
async fn check_span_ids_exist_in_db_returning_missing(
    con: &PgPool,
    span_ids_to_check: &HashSet<u64>,
    trace_id: u64,
    instance_id: &InstanceId,
) -> Result<HashSet<u64>, sqlx::Error> {
    if span_ids_to_check.is_empty() {
        debug!("Span ids to check is empty, returning empty list");
        return Ok(HashSet::new());
    }
    let as_vec: Vec<i64> = span_ids_to_check.iter().map(|e| *e as i64).collect();
    debug!("Getting {} span ids from the db", as_vec.len());
    trace!("Span ids: {:?}", as_vec);
    let res: Vec<i64> = sqlx::query_scalar!(
        "select id from span where trace_id=$1 and instance_id=$2 and id = ANY($3::BIGINT[])",
        trace_id as i64,
        instance_id.instance_id,
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
    instance_id: &InstanceId,
    trace_id: u64,
    top_level_span_name: &str,
    timestamp: u64,
) -> Result<(), backtraced_error::SqlxError> {
    info!(
        "Inserting trace header information for: {}",
        top_level_span_name
    );
    let now_nanos = now_nanos_u64();
    if let Err(e) = sqlx::query!(
        "insert into trace (env, service_name, instance_id, id, top_level_span_name, timestamp, updated_at) values
        ($1, $2, $3, $4, $5, $6, $7);",
        instance_id.service_id.env.to_string() as _,
        instance_id.service_id.name as _,
        instance_id.instance_id as _,
        trace_id as i64,
        top_level_span_name as _,
        timestamp as i64,
        now_nanos as i64
    )
        .execute(con.deref_mut())
        .await
    {
        return Err(SqlxError::from_sqlx_error(e, format!("DB Error when inserting trace with data:\
         instance_id={instance_id:?} trace_id={trace_id} \
         timestamp={timestamp} top_level_span_name={top_level_span_name} updated_at={now_nanos}")));
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
pub async fn get_existing_span_ids(
    con: &mut Transaction<'static, Postgres>,
    instance_id: i64,
    trace_id: i64,
    span_ids: &[i64],
) -> Result<Vec<i64>, backtraced_error::SqlxError> {
    sqlx::query_scalar!(
        "select id from span where instance_id=$1 and trace_id=$2 and id = ANY($3::BIGINT[])",
        instance_id,
        trace_id,
        span_ids
    )
    .fetch_all(con.deref_mut())
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, "getting existing span ids"))
}

#[instrument(skip_all)]
pub async fn update_trace_with_new_state(
    con: &PgPool,
    instance_id: &InstanceId,
    fragment: TraceState,
) -> Result<(), SqlxError> {
    trace!("fragment = {:#?}", fragment);
    info!("fragment for: {}", fragment.root_span.name);

    let db_trace_duration = get_db_trace(&con, &instance_id, fragment.root_span.id).await?;
    let trace_already_exists = db_trace_duration.is_some();
    debug!("trace_already_exists = {trace_already_exists}");
    let trace_is_complete = db_trace_duration
        .as_ref()
        .map(|t| t.duration.is_some())
        .unwrap_or(false);
    debug!("trace_is_complete = {trace_is_complete}");
    if trace_is_complete {
        error!("Got new data for completed trace");
        return Ok(());
    }
    debug!("Trying to start transaction");
    let mut transaction = con
        .begin()
        .instrument(info_span!("start_transaction"))
        .await
        .map_err(|e| SqlxError::from_sqlx_error(e, "starting transaction"))?;
    debug!("Started!");
    if !trace_already_exists {
        insert_new_trace(
            &mut transaction,
            &instance_id,
            fragment.root_span.id,
            &fragment.root_span.name.clone(),
            fragment.root_span.timestamp,
        )
        .await?;
        insert_spans(
            &mut transaction,
            &vec![NewSpan {
                id: fragment.root_span.id,
                name: fragment.root_span.name.clone(),
                timestamp: fragment.root_span.timestamp,
                duration: fragment.root_span.duration,
                parent_id: None,
                key_vals: fragment.root_span.key_vals.clone(),
                location: fragment.root_span.location.clone(),
            }],
            fragment.root_span.id,
            instance_id,
        )
        .await?;
    }
    let open_spans = fragment.open_spans.clone();
    let open_spans_ids: Vec<i64> = open_spans.keys().map(|e| (*e) as i64).collect();
    let existing_span_id = get_existing_span_ids(
        &mut transaction,
        instance_id.instance_id,
        fragment.root_span.id as i64,
        &open_spans_ids,
    )
    .await?;
    let mut spans_to_insert = vec![];
    for open_span in open_spans.values() {
        if !existing_span_id.contains(&(open_span.id as i64)) {
            spans_to_insert.push(NewSpan {
                id: open_span.id,
                name: open_span.name.clone(),
                timestamp: open_span.timestamp,
                duration: None,
                parent_id: Some(open_span.parent_id),
                key_vals: open_span.key_vals.clone(),
                location: open_span.location.clone(),
            });
        }
    }
    for s in &fragment.closed_spans {
        spans_to_insert.push(NewSpan {
            id: s.id,
            name: s.name.clone(),
            timestamp: s.timestamp,
            duration: Some(s.duration),
            parent_id: Some(s.parent_id),
            key_vals: s.key_vals.clone(),
            location: s.location.clone(),
        });
    }
    insert_spans(
        &mut transaction,
        &spans_to_insert,
        fragment.root_span.id,
        &instance_id,
    )
    .await?;
    crate::api::database::insert_events(
        &mut transaction,
        &fragment.new_events,
        fragment.root_span.id,
        &instance_id,
    )
    .await?;
    /*
        spans_produced
        events_produced
        events_dropped_by_sampling
    */
    let spans_produced = fragment.spans_produced as u64;
    let events_produced = fragment.events_produced as u64;
    let events_dropped_by_sampling = fragment.events_dropped_by_sampling as u64;
    let stored_span_count_increase = spans_to_insert.len() as u64;
    let stored_event_count_increase = fragment.new_events.len() as u64;
    let size_bytes_increase = fragment.total_size();
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
    spans_produced={spans_produced}
    events_produced={events_produced}
    events_dropped_by_sampling={events_dropped_by_sampling}
    stored_span_count_increase={stored_span_count_increase}
    stored_event_count_increase={stored_event_count_increase}
    size_bytes_increase={size_bytes_increase}
    warnings_count_increase={warnings_count_increase}
    has_errors={has_errors}",
        fragment.root_span.duration
    );
    update_trace_header(
        &mut transaction,
        &instance_id,
        fragment.root_span.id,
        fragment.root_span.duration,
        spans_produced,
        stored_span_count_increase,
        events_produced,
        events_dropped_by_sampling,
        stored_event_count_increase,
        size_bytes_increase as u64,
        warnings_count_increase,
        has_errors,
    )
    .await?;
    transaction
        .commit()
        .await
        .map_err(|e| SqlxError::from_sqlx_error(e, "commiting transaction"))?;
    Ok(())
}

// #[instrument(skip_all)]
// async fn update_closed_spans(
//     con: &PgPool,
//     instance_id: &InstanceId,
//     closed_spans: &[OldClosedSpan],
// ) {
//     info!("{} spans to close", closed_spans.len());
//     for span in closed_spans {
//         debug!("Closing span: {:?}", span);
//         let res: Result<PgQueryResult, sqlx::Error> = sqlx::query!(
//             "update span set duration=$1 where instance_id=$2 and trace_id=$3 and id=$4;",
//             span.duration as i64,
//             instance_id.instance_id,
//             span.trace_id as i64,
//             span.span_id as i64,
//         )
//         .execute(con)
//         .await;
//         match res {
//             Ok(res) => {
//                 debug!("Updated ({} rows)", res.rows_affected());
//             }
//             Err(err) => {
//                 error!("Error closing span {err:?} {span:?}");
//             }
//         }
//         if span.span_id == span.trace_id {
//             info!("Span was root, updating trace duration");
//             let res = sqlx::query!(
//                 "update trace set duration=$1 where instance_id=$2 and id=$3",
//                 span.duration as i64,
//                 instance_id.instance_id,
//                 span.trace_id as i64
//             )
//             .execute(con)
//             .await;
//             match res {
//                 Ok(res) => {
//                     debug!("Updated ({} rows)", res.rows_affected());
//                 }
//                 Err(err) => {
//                     error!(
//                         "Error updating trace {err:?} duration={} instance_id={:?} id={}",
//                         span.duration, instance_id, span.trace_id
//                     );
//                 }
//             }
//         }
//     }
// }

#[instrument(skip_all)]
pub async fn insert_orphan_events(
    con: &PgPool,
    instance_id: &InstanceId,
    orphan_events: &[NewOrphanEvent],
) {
    info!("{} events to insert", orphan_events.len());
    for e in orphan_events {
        trace!("Inserting event: {:?}", e);
    }
    let mut timestamps = vec![];
    let mut envs = vec![];
    let mut service_names = vec![];
    let mut severities = vec![];
    let mut message = vec![];
    for event in orphan_events {
        timestamps.push(event.timestamp as i64);
        envs.push(instance_id.service_id.env.to_string());
        service_names.push(instance_id.service_id.name.as_str());
        severities.push(Severity::from(event.severity));
        message.push(event.message.clone());
    }
    let orphan_events_db_ids = match sqlx::query_scalar!(
            "insert into orphan_event (timestamp, env, service_name, severity, message) select * from unnest($1::BIGINT[], \
            $2::TEXT[], $3::TEXT[], $4::severity_level[], $5::TEXT[]) returning id;",
            &timestamps,
            &envs,
            &service_names as &Vec<&str>,
            severities.as_slice() as &[Severity],
            &message as &Vec<Option<String>>
        )
        .fetch_all(con).await {
        Ok(res) => {
            debug!("Inserted and got {} ids back", res.len());
            let res: Vec<i64> = res;
            res
        }
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
    let mut kv_envs = vec![];
    let mut kv_orphan_key = vec![];
    let mut kv_orphan_value = vec![];
    let mut kv_service_names = vec![];

    for (idx, event) in orphan_events.iter().enumerate() {
        for (key, val) in &event.key_vals {
            kv_orphan_event_id.push(orphan_events_db_ids[idx]);
            kv_orphan_timestamp.push(event.timestamp as i64);
            kv_envs.push(instance_id.service_id.env.to_string());
            kv_service_names.push(instance_id.service_id.name.as_str());
            kv_orphan_key.push(key.as_str());
            kv_orphan_value.push(val.as_str());
        }
    }
    info!("{} events key values to insert", kv_orphan_event_id.len());

    match sqlx::query!(
            "insert into orphan_event_key_value (orphan_event_id, key, value) select * from unnest($1::BIGINT[], $2::TEXT[], $3::TEXT[]);",
            &kv_orphan_event_id,
            &kv_orphan_key as &Vec<&str>,
            &kv_orphan_value as &Vec<&str>,
        )
        .execute(con).await {
        Ok(res) => {
            debug!("Inserted {}", res.rows_affected());
        }
        Err(e) => {
            error!("Error inserting orphan events: {:#?}", e);
            error!("kv_orphan_event_id={:?}", kv_orphan_event_id);
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
fn truncate_open_span_key_values_if_needed(spans: &mut ValuesMut<u64, OpenSpan>) {
    for s in spans {
        truncate_key_values(&mut s.key_vals);
    }
}
#[instrument(skip_all)]
fn truncate_closed_span_key_values_if_needed(spans: &mut Vec<ClosedSpan>) {
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
fn truncate_orphan_events_and_kv_if_needed(events: &mut Vec<NewOrphanEvent>) {
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

// #[debug_handler]
#[instrument(level = "error", skip_all, err(Debug))]
pub async fn instance_update_post(
    State(app_state): State<AppState>,
    trace_data: Json<ExportedServiceTraceData>,
) -> Result<Json<Sampling>, ApiError> {
    let con = app_state.con;
    let trace_data: ExportedServiceTraceData = trace_data.0;
    trace!("{trace_data:#?}");
    let sampling = update_service_and_instance_data(&app_state.services_runtime_stats, &trace_data)
        .map_err(|_e| {
            error!("Tried to update instance, but was not registered!");
            ApiError {
                code: StatusCode::BAD_REQUEST,
                message: "Instance not registered by SSE".to_string(),
            }
        })?;
    let instance_id = trace_data.instance_id;
    info!("Got {} new fragments", trace_data.traces_state.len());
    for mut fragment in trace_data.traces_state.into_values() {
        truncate_key_values(&mut fragment.root_span.key_vals);
        truncate_open_span_key_values_if_needed(&mut fragment.open_spans.values_mut());
        truncate_closed_span_key_values_if_needed(&mut fragment.closed_spans);
        truncate_events_if_needed(&mut fragment.new_events);
        if let Err(db_error) = update_trace_with_new_state(&con, &instance_id, fragment).await {
            error!("DB error when inserting fragment: {:#?}", db_error);
        }
    }

    let mut orphan_events = trace_data.orphan_events;
    truncate_orphan_events_and_kv_if_needed(&mut orphan_events);
    insert_orphan_events(&con, &instance_id, &orphan_events).await;

    Ok(Json(sampling))
}

#[instrument(skip_all)]
async fn update_trace_header(
    con: &mut Transaction<'static, Postgres>,
    instance_id: &InstanceId,
    trace_id: u64,
    duration: Option<u64>,
    spans_produced: u64,
    spans_stored_increase: u64,
    events_produced: u64,
    events_dropped_by_sampling: u64,
    stored_event_count_increase: u64,
    size_bytes_increase: u64,
    warnings_count_increase: u64,
    has_new_errors: bool,
) -> Result<(), SqlxError> {
    sqlx::query!(
        "update trace
        set duration=$3,
            spans_produced=$4,
            spans_stored=($5 + spans_stored),
            events_produced=$6,
            events_dropped_by_sampling=$7,
            events_stored=($8 + events_stored),
            size_bytes=(size_bytes + $9),
            warnings=(warnings + $10),
            has_errors=(has_errors or $11)
        where instance_id = $1
          and id = $2;",
        instance_id.instance_id,                   //1
        trace_id as i64 as _,                      //2
        duration.map(|d| d as i64) as Option<i64>, //3
        spans_produced as i64 as _,                //4
        spans_stored_increase as i64 as _,         //5
        events_produced as i64 as _,               //6
        events_dropped_by_sampling as i64 as _,    //7
        stored_event_count_increase as i64 as _,   //8
        size_bytes_increase as i64 as _,           //9
        warnings_count_increase as i64 as _,       //10
        has_new_errors,                            //11
    )
    .execute(con.deref_mut())
    .await
    .map_err(|e| SqlxError::from_sqlx_error(e, "updating trace header"))?;
    Ok(())
}

#[instrument(skip_all)]
pub(crate) async fn insert_spans(
    con: &mut Transaction<'static, Postgres>,
    new_spans: &[NewSpan],
    trace_id: u64,
    instance_id: &InstanceId,
) -> Result<(), backtraced_error::SqlxError> {
    if new_spans.is_empty() {
        info!("No spans to insert");
        return Ok(());
    } else {
        info!("Inserting {} spans", new_spans.len());
    }
    let span_ids: Vec<i64> = new_spans.iter().map(|s| s.id as i64).collect();
    let instance_ids: Vec<i64> = new_spans.iter().map(|_s| instance_id.instance_id).collect();
    let trace_ids: Vec<i64> = new_spans.iter().map(|_s| trace_id as i64).collect();
    let timestamp: Vec<i64> = new_spans.iter().map(|s| s.timestamp as i64).collect();
    let modules: Vec<Option<String>> = new_spans
        .iter()
        .map(|s| s.location.module.clone())
        .collect();
    let filenames: Vec<Option<String>> = new_spans
        .iter()
        .map(|s| s.location.filename.clone())
        .collect();
    let lines: Vec<Option<i32>> = new_spans
        .iter()
        .map(|s| s.location.line.map(|l| l as i32))
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
    // on conflict can happen if the span was active and open (so exists), but now is closed
    match sqlx::query!(
            "insert into span (id, instance_id, trace_id, timestamp, parent_id, duration, name, module, filename, line)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::BIGINT[], $6::BIGINT[], $7::TEXT[], $8::TEXT[], $9::TEXT[], $10::INT[])
             on conflict (instance_id, trace_id, id) do update set duration=excluded.duration;",
            &span_ids,
            &instance_ids,
            &trace_ids,
            &timestamp,
            &parent_id as &Vec<Option<i64>>,
            &duration as &Vec<Option<i64>>,
            &name,
            &modules as &Vec<Option<String>>,
            &filenames as &Vec<Option<String>>,
            &lines as &Vec<Option<i32>>,
        )
        .execute(con.deref_mut())
        .await {
        Ok(_) => {
            info!("Inserted spans");
        }
        Err(e) => {
            error!("Error when inserting spans");
            error!("Span Ids: {:?}", span_ids);
            error!("Instance Ids: {:?}", instance_ids);
            error!("Trace Ids: {:?}", trace_id);
            error!("Timestamp: {:?}", timestamp);
            error!("parent_id: {:?}", parent_id);
            error!("duration: {:?}", duration);
            error!("name: {:?}", name);
            return Err(SqlxError::from_sqlx_error(e, "inserting spans"));

        }
    };
    let mut kv_instance_id = vec![];
    let mut kv_trace_id = vec![];
    let mut kv_span_id = vec![];
    let mut kv_key = vec![];
    let mut kv_value = vec![];
    for (_idx, span) in new_spans.iter().enumerate() {
        for (key, val) in &span.key_vals {
            kv_instance_id.push(instance_id.instance_id);
            kv_trace_id.push(trace_id as i64);
            kv_span_id.push(span.id as i64);
            kv_key.push(key.as_str());
            kv_value.push(val.as_str());
        }
    }
    if kv_instance_id.is_empty() {
        info!("No span key-values to insert");
        return Ok(());
    } else {
        info!("Inserting {} span key-values", kv_instance_id.len());
    }
    match sqlx::query!(
            "insert into span_key_value (instance_id, trace_id, span_id,  key, value)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::TEXT[], $5::TEXT[]);",
            &kv_instance_id,
            &kv_trace_id,
            &kv_span_id,
            &kv_key as &Vec<&str>,
            &kv_value as &Vec<&str>
        )
        .execute(con.deref_mut())
        .await {
        Ok(_) => {
            info!("Inserted span key-values");
        }
        Err(e) => {
            error!("Error when inserting span key-values");
            error!("kv_instance_id: {:?}", kv_instance_id);
            error!("kv_trace_id: {:?}", kv_trace_id);
            error!("kv_span_id: {:?}", kv_span_id);
            error!("kv_key: {:?}", kv_key);
            error!("kv_value: {:?}", kv_value);
            return Err(SqlxError::from_sqlx_error(e, "inserting spans kvs"));
        }
    };

    Ok(())
}
