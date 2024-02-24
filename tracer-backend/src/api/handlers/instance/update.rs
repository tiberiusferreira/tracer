use crate::api::handlers::Severity;
use crate::api::state::{AppState, BytesBudgetUsage, ServiceDataPoint, Shared};
use crate::api::ApiError;
use crate::{
    MAX_STATS_HISTORY_DATA_COUNT, SINGLE_KEY_VALUE_KEY_CHARS_LIMIT,
    SINGLE_KEY_VALUE_VALUE_CHARS_LIMIT,
};
use api_structs::instance::update::{
    ClosedSpan, ExportedServiceTraceData, NewOrphanEvent, NewSpan, NewSpanEvent, Sampling,
    SamplingData, TraceFragment,
};
use api_structs::time_conversion::now_nanos_u64;
use api_structs::ui::service::{OrphanEvent, ProfileData, TraceHeader};
use api_structs::{InstanceId, ServiceId, TraceName};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use sqlx::postgres::PgQueryResult;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::ops::DerefMut;
use tracing::{debug, error, info, info_span, instrument, trace, Instrument};

pub struct ServiceNotRegisteredError;

#[instrument(skip_all)]
fn update_service_and_instance_data(
    live_instances: &Shared<HashMap<ServiceId, crate::api::state::ServiceRuntimeData>>,
    exported_service_trace_data: &ExportedServiceTraceData,
) -> Result<Sampling, ServiceNotRegisteredError> {
    let mut w_lock = live_instances.write();
    let service_data = match w_lock.get_mut(&ServiceId {
        name: exported_service_trace_data
            .instance_id
            .service_id
            .name
            .clone(),
        env: exported_service_trace_data
            .instance_id
            .service_id
            .env
            .clone(),
    }) {
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
    let mut received_bytes_per_trace: HashMap<TraceName, u64> = HashMap::new();
    let mut active_traces = vec![];
    let mut finished_traces = vec![];
    let mut received_orphan_event_bytes = 0;
    for e in &exported_service_trace_data.orphan_events {
        received_orphan_event_bytes += e.message.as_ref().map(|m| m.len()).unwrap_or(0);
        for (k, v) in &e.key_vals {
            received_orphan_event_bytes += k.len();
            received_orphan_event_bytes += v.len();
        }
    }
    for trace_frag in exported_service_trace_data.active_trace_fragments.values() {
        let mut received_trace_bytes = 0;
        for s in &trace_frag.new_spans {
            received_trace_bytes += s.name.len();
            for (k, v) in &s.key_vals {
                received_trace_bytes += k.len();
                received_trace_bytes += v.len();
            }
        }
        for e in &trace_frag.new_events {
            received_trace_bytes += e.message.as_ref().map(|m| m.len()).unwrap_or(0);
            for (k, v) in &e.key_vals {
                received_trace_bytes += k.len();
                received_trace_bytes += v.len();
            }
        }
        let entry = received_bytes_per_trace
            .entry(trace_frag.trace_name.clone())
            .or_default();
        *entry += received_trace_bytes as u64;
        let header = TraceHeader {
            trace_id: trace_frag.trace_id,
            trace_name: trace_frag.trace_name.clone(),
            trace_timestamp: trace_frag.trace_timestamp,
            new_warnings: trace_frag.has_warnings(),
            new_errors: trace_frag.has_errors(),
            fragment_bytes: received_trace_bytes as u64,
            duration: trace_frag.duration_if_closed(&exported_service_trace_data.closed_spans),
        };
        if header.duration.is_some() {
            finished_traces.push(header);
        } else {
            active_traces.push(header);
        }
    }
    let now_nanos = now_nanos_u64();
    // updating instance
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

    let mut last_bytes_budget = service_data
        .service_data_points
        .back()
        .map(|b| b.budget_usage.clone())
        .unwrap_or_else(|| BytesBudgetUsage::new(60, 10_000));
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
            .traces_usage
            .iter()
            .map(|(name, _usage)| {
                let sampling = if remaining_budget.is_trace_over_budget(name) {
                    SamplingData {
                        new_traces_sampling_rate_0_to_1: 0.0,
                        existing_traces_new_data_sampling_rate_0_to_1: 1.0,
                    }
                } else {
                    SamplingData {
                        new_traces_sampling_rate_0_to_1: 1.0,
                        existing_traces_new_data_sampling_rate_0_to_1: 1.0,
                    }
                };
                (name.to_string(), sampling)
            })
            .collect::<HashMap<TraceName, SamplingData>>(),
        orphan_events_sampling_rate_0_to_1: {
            if remaining_budget.is_orphan_events_over_budget() {
                0.
            } else {
                1.
            }
        },
    };
    debug!("new_sampling = {new_sampling:?}");
    let new = ServiceDataPoint {
        timestamp: now_nanos,
        instance_id: exported_service_trace_data.instance_id.instance_id,
        export_buffer_stats: exported_service_trace_data.producer_stats.clone(),
        active_traces,
        finished_traces,
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
) -> Result<Option<TraceDuration>, sqlx::Error> {
    debug!("instance_id: {instance_id:?}, trace_id: {trace_id}");
    let raw: Option<RawTraceHeader> = sqlx::query_as!(
        RawTraceHeader,
        "select duration from trace where instance_id=$1 and id=$2",
        instance_id.instance_id as i64,
        trace_id as i64
    )
    .fetch_optional(con)
    .await?;
    return match raw {
        None => Ok(None),
        Some(raw) => Ok(Some(TraceDuration {
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
) -> Result<(), sqlx::Error> {
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
        error!(
            "DB Error when inserting trace with data:\
         instance_id={instance_id:?} trace_id={trace_id} \
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
    instance_id: &InstanceId,
    mut fragment: TraceFragment,
) -> Result<(), sqlx::Error> {
    info!("fragment for: {}", fragment.trace_name);
    fragment.new_events.sort_by_key(|e| e.timestamp);
    fragment.new_spans.sort_by_key(|e| e.timestamp);
    let db_trace = get_db_trace(&con, &instance_id, fragment.trace_id).await?;
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
        &instance_id,
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
            &instance_id,
            fragment.trace_id,
            &fragment.trace_name,
            fragment.trace_timestamp,
        )
        .await?;
    }
    insert_spans(
        &mut transaction,
        &fragment.new_spans,
        fragment.trace_id,
        &instance_id,
        &relocated_span_ids,
    )
    .await?;
    crate::api::database::insert_events(
        &mut transaction,
        &fragment.new_events,
        fragment.trace_id,
        &instance_id,
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
        &instance_id,
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
async fn update_closed_spans(con: &PgPool, instance_id: &InstanceId, closed_spans: &[ClosedSpan]) {
    info!("{} spans to close", closed_spans.len());
    for span in closed_spans {
        debug!("Closing span: {:?}", span);
        let res: Result<PgQueryResult, sqlx::Error> = sqlx::query!(
            "update span set duration=$1 where instance_id=$2 and trace_id=$3 and id=$4;",
            span.duration as i64,
            instance_id.instance_id,
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
                "update trace set duration=$1 where instance_id=$2 and id=$3",
                span.duration as i64,
                instance_id.instance_id,
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
                        "Error updating trace {err:?} duration={} instance_id={:?} id={}",
                        span.duration, instance_id, span.trace_id
                    );
                }
            }
        }
    }
}

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

#[instrument(level = "error", skip_all, err(Debug))]
pub async fn instance_update_post(
    State(app_state): State<AppState>,
    compressed_json: axum::body::Bytes,
) -> Result<Json<Sampling>, ApiError> {
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
    let sampling = update_service_and_instance_data(&app_state.services_runtime_stats, &trace_data)
        .map_err(|_e| {
            error!("Tried to update instance, but was not registered!");
            ApiError {
                code: StatusCode::BAD_REQUEST,
                message: "Instance not registered by SSE".to_string(),
            }
        })?;
    let instance_id = trace_data.instance_id;
    info!(
        "Got {} new fragments",
        trace_data.active_trace_fragments.len()
    );
    for mut fragment in trace_data.active_trace_fragments.into_values() {
        if fragment.new_events.is_empty() && fragment.new_spans.is_empty() {
            continue;
        }
        truncate_span_key_values_if_needed(&mut fragment.new_spans);
        truncate_events_if_needed(&mut fragment.new_events);
        if let Err(db_error) = update_trace_with_new_fragment(&con, &instance_id, fragment).await {
            error!("DB error when inserting fragment: {:#?}", db_error);
        }
    }

    update_closed_spans(&con, &instance_id, &trace_data.closed_spans).await;
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
    original_span_count: u64,
    original_event_count: u64,
    stored_span_count_increase: u64,
    stored_event_count_increase: u64,
    estimated_size_bytes: u64,
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
            estimated_size_bytes=(estimated_size_bytes + $8),
            warning_count=(warning_count + $9),
            has_errors=(has_errors or $10)
        where instance_id = $1
          and id = $2;",
        instance_id.instance_id,
        trace_id as i64 as _,
        duration.map(|d| d as i64) as Option<i64>,
        original_span_count as i64 as _,
        original_event_count as i64 as _,
        stored_span_count_increase as i64 as _,
        stored_event_count_increase as i64 as _,
        estimated_size_bytes as i64 as _,
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
    instance_id: &InstanceId,
    relocated_span_ids: &HashSet<u64>,
) -> Result<(), sqlx::Error> {
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
            "insert into span (id, instance_id, trace_id, timestamp, parent_id, duration, name, relocated)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::BIGINT[], $6::BIGINT[], $7::TEXT[], $8::BOOLEAN[]);",
            &span_ids,
            &instance_ids,
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
        }
        Err(e) => {
            error!("Error when inserting spans");
            error!("Span Ids: {:?}", span_ids);
            error!("Instance Ids: {:?}", instance_ids);
            error!("Trace Ids: {:?}", trace_id);
            error!("Timestamp: {:?}", timestamp);
            error!("Relocated: {:?}", relocated);
            error!("parent_id: {:?}", parent_id);
            error!("duration: {:?}", duration);
            error!("name: {:?}", name);
            return Err(e);
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
            return Err(e);
        }
    };

    Ok(())
}
