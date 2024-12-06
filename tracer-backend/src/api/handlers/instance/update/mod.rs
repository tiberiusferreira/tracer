use std::collections::HashMap;

use api_structs::instance::update::{ConfigChange, Event, InstanceSnapshot, Span, TraceSnapshot};
use api_structs::InstanceGlobalId;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use base64::Engine;
use sqlx::{PgPool, Postgres, Transaction};
use tracing::{error, info, instrument, trace};
use tracked_error::SqlxError;

use crate::api::state::AppState;
use crate::api::ApiError;
use crate::{SINGLE_KEY_VALUE_KEY_CHARS_LIMIT, SINGLE_KEY_VALUE_VALUE_CHARS_LIMIT};

mod db_trace;
mod instance;
mod trace;
pub struct ServiceNotRegisteredError;

#[instrument(skip_all)]
fn update_service_and_instance_data(// live_instances: &Shared<HashMap<ServiceId, crate::api::state::ServiceRuntimeData>>,
                                     // exported_service_trace_data: &ExportedServiceTraceData,
) -> Result<(), ServiceNotRegisteredError> {
    unimplemented!()
    // let mut w_lock = live_instances.write();
    // let service_data = match w_lock.get_mut(&exported_service_trace_data.instance_id.service_id) {
    //     None => return Err(ServiceNotRegisteredError),
    //     Some(service_data) => service_data,
    // };
    // let instance = match service_data
    //     .instances
    //     .get_mut(&exported_service_trace_data.instance_id.instance_id)
    // {
    //     None => return Err(ServiceNotRegisteredError),
    //     Some(instance) => instance,
    // };
    // {
    //     instance.rust_log = exported_service_trace_data.rust_log.clone();
    //     instance.last_seen = std::time::Instant::now();
    //     if let Some(profile_data) = &exported_service_trace_data.profile_data {
    //         instance.profile_data = Some(ProfileData {
    //             profile_data_timestamp: now_nanos_u64(),
    //             profile_data: profile_data.clone(),
    //         });
    //     }
    // }
    //
    // let mut traces_header = vec![];
    // let mut received_bytes_per_trace: HashMap<TraceName, u64> = HashMap::new();
    // let received_orphan_event_bytes = exported_service_trace_data.orphan_events_size();
    // for trace_state in exported_service_trace_data.traces_state.values() {
    //     let received_trace_bytes = trace_state.total_size();
    //     let root = trace_state.root();
    //     let entry = received_bytes_per_trace
    //         .entry(root.name.clone())
    //         .or_default();
    //     *entry += received_trace_bytes as u64;
    //     let header = TraceHeader {
    //         trace_id: trace_state.id,
    //         trace_name: root.name.clone(),
    //         trace_timestamp: root.timestamp,
    //         new_warnings: trace_state.has_warnings(),
    //         new_errors: trace_state.has_errors(),
    //         fragment_bytes: received_trace_bytes as u64,
    //         is_closed: trace_state.is_closed(),
    //         duration: root.duration,
    //     };
    //     traces_header.push(header);
    // }
    // let mut last_bytes_budget = service_data
    //     .service_data_points
    //     .back()
    //     .map(|b| b.budget_usage.clone())
    //     .unwrap_or_else(|| BytesBudgetUsage::new(60, 100_000));
    // debug!("Previous budget: {last_bytes_budget:?}");
    // last_bytes_budget.update();
    // debug!("Previous budget after update: {last_bytes_budget:?}");
    //
    // debug!("Decreasing orphan events budget by {received_orphan_event_bytes}");
    // last_bytes_budget.increase_orphan_events_usage_by(received_orphan_event_bytes as u32);
    // for (trace_name, received_bytes) in &received_bytes_per_trace {
    //     debug!("Decreasing {trace_name} budget by {received_bytes}");
    //     last_bytes_budget.increase_trace_usage_by(trace_name, *received_bytes as u32);
    // }
    // let remaining_budget = last_bytes_budget;
    // debug!("remaining_budget = {remaining_budget:?}");
    // let new_sampling = Sampling {
    //     traces: remaining_budget
    //         .traces_usage_bytes
    //         .iter()
    //         .map(|(name, _usage)| {
    //             let sampling = if remaining_budget.is_trace_over_budget(name) {
    //                 SamplingState::DropNewTracesKeepExistingTraceNewData
    //             } else {
    //                 SamplingState::AllowNewTraces
    //             };
    //             (name.to_string(), sampling)
    //         })
    //         .collect::<HashMap<TraceName, SamplingState>>(),
    //     allow_new_orphan_events: !remaining_budget.is_orphan_events_over_budget(),
    // };
    // debug!("new_sampling = {new_sampling:?}");
    // let new = ServiceDataPoint {
    //     timestamp: now_nanos_u64(),
    //     instance_id: exported_service_trace_data.instance_id.instance_id,
    //     traces: traces_header,
    //     orphan_events: exported_service_trace_data
    //         .orphan_events
    //         .iter()
    //         .map(|e| OrphanEvent {
    //             timestamp: e.timestamp,
    //             severity: e.severity,
    //             message: e.message.clone(),
    //             key_vals: e.key_vals.clone(),
    //             location: e.location.clone(),
    //         })
    //         .collect(),
    //     budget_usage: remaining_budget,
    // };
    // service_data.service_data_points.push_back(new);
    // while service_data.service_data_points.len() > MAX_STATS_HISTORY_DATA_COUNT {
    //     service_data.service_data_points.pop_front();
    // }
    // trace!("{:?}", new_sampling);
    // Ok(new_sampling)
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
    instance_id: &InstanceGlobalId,
    trace_id: u64,
) -> Result<Option<TraceDuration>, tracked_error::SqlxError> {
    // debug!("instance_id: {instance_id:?}, trace_id: {trace_id}");
    // let raw: Option<RawTraceHeader> = sqlx::query_as!(
    //     RawTraceHeader,
    //     "select duration from trace where instance_id=$1 and id=$2",
    //     instance_id.instance_id as i64,
    //     trace_id as i64
    // )
    // .fetch_optional(con)
    // .await
    // .map_err(|e| SqlxError::from_sqlx_error(e, "get_db_trace"))?;
    // return match raw {
    //     None => Ok(None),
    //     Some(raw) => Ok(Some(TraceDuration {
    //         duration: raw.duration.map(|e| e as u64),
    //     })),
    // };
    unimplemented!()
}

// #[instrument(skip_all)]
// async fn check_span_ids_exist_in_db_returning_missing(
//     con: &PgPool,
//     span_ids_to_check: &HashSet<u64>,
//     trace_id: u64,
//     instance_id: &InstanceId,
// ) -> Result<HashSet<u64>, sqlx::Error> {
//     if span_ids_to_check.is_empty() {
//         debug!("Span ids to check is empty, returning empty list");
//         return Ok(HashSet::new());
//     }
//     let as_vec: Vec<i64> = span_ids_to_check.iter().map(|e| *e as i64).collect();
//     debug!("Getting {} span ids from the db", as_vec.len());
//     trace!("Span ids: {:?}", as_vec);
//     let res: Vec<i64> = sqlx::query_scalar!(
//         "select id from span where trace_id=$1 and instance_id=$2 and id = ANY($3::BIGINT[])",
//         trace_id as i64,
//         instance_id.instance_id,
//         as_vec.as_slice()
//     )
//     .fetch_all(con)
//     .await?;
//     debug!("Got {} back", res.len());
//     trace!("Span ids from DB {:?}", res);
//     let existing_ids: HashSet<u64> = res.iter().map(|id| *id as u64).collect();
//     let missing_ids: HashSet<u64> = span_ids_to_check
//         .difference(&existing_ids)
//         .cloned()
//         .collect();
//     Ok(missing_ids)
// }

#[instrument(skip_all)]
pub async fn get_existing_span_ids(
    con: &mut Transaction<'static, Postgres>,
    instance_id: i64,
    trace_id: i64,
    span_ids: &[i64],
) -> Result<Vec<i64>, tracked_error::SqlxError> {
    // sqlx::query_scalar!(
    //     "select id from span where instance_id=$1 and trace_id=$2 and id = ANY($3::BIGINT[])",
    //     instance_id,
    //     trace_id,
    //     span_ids
    // )
    // .fetch_all(con.deref_mut())
    // .await
    // .map_err(|e| SqlxError::from_sqlx_error(e, "getting existing span ids"))
    unimplemented!()
}

#[instrument(skip_all)]
pub async fn update_trace_with_new_state(
    con: &PgPool,
    instance_id: &InstanceGlobalId,
    trace_state: TraceSnapshot,
) -> Result<(), SqlxError> {
    trace!("fragment = {:#?}", trace_state);
    info!("fragment for: {}", trace_state.root().name);
    // For a given trace we can never have:
    // 1. A parent closed with an open child
    // 2. A child with a longer duration than the parent
    // 3. More than one root span
    let trace_id = trace_state.trace_id;
    // let mut transaction = con
    //     .begin()
    //     .instrument(info_span!("starting_transaction"))
    //     .await?;
    // let db_trace =
    //     db_trace::get_trace_header(&mut transaction, instance_id.instance_id, trace_id as i32)
    //         .await?;
    // match db_trace {
    //     None => {
    //         db_trace::insert_new_trace(
    //             &mut transaction,
    //             instance_id,
    //             trace_id as i32,
    //             trace_state.spans_produced as i32,
    //             trace_state.events_produced as i32,
    //             trace_state.events_dropped_by_sampling as i32,
    //         )
    //         .await?
    //     }
    //     Some(existing) => {
    //         if trace_state.events_produced < existing.events_produced as u32 {
    //             error!("Events produced dropped!");
    //         }
    //         if trace_state.events_dropped_by_sampling < existing.events_dropped_by_sampling as u32 {
    //             error!("Event dropped by sampling dropped!");
    //         }
    //         if trace_state.spans_produced < existing.spans_produced as u32 {
    //             error!("Spans produced dropped!");
    //         }
    //         db_trace::update_trace_header(
    //             &mut transaction,
    //             instance_id,
    //             trace_id as i32,
    //             trace_state.spans_produced as i32,
    //             trace_state.events_produced as i32,
    //             trace_state.events_dropped_by_sampling as i32,
    //         )
    //         .await?;
    //     }
    // }
    // let trace_root = trace_state.root();
    // let spans: Vec<Span> = trace_state.spans.values().into_iter().cloned().collect();
    //
    // insert_spans(&mut transaction, &spans, trace_id as i32, instance_id).await?;
    //
    // db_trace::upsert_trace_cache(
    //     &mut transaction,
    //     instance_id,
    //     trace_id as i32,
    //     time_from_nanos(trace_root.timestamp),
    //     &trace_root.name,
    //     trace_root.duration as i64,
    //     0,
    //     0,
    //     0,
    //     0,
    //     false,
    //     trace_root.is_closed,
    // )
    // .await?;
    //
    // crate::api::database::insert_events(
    //     &mut transaction,
    //     &trace_state.new_events,
    //     trace_id as i32,
    //     &instance_id,
    // )
    // .await?;

    // let db_trace_duration = get_db_trace(&con, &instance_id, fragment.root_span.id).await?;
    // let trace_already_exists = db_trace_duration.is_some();
    // debug!("trace_already_exists = {trace_already_exists}");
    // let trace_is_complete = db_trace_duration
    //     .as_ref()
    //     .map(|t| t.duration.is_some())
    //     .unwrap_or(false);
    // debug!("trace_is_complete = {trace_is_complete}");
    // if trace_is_complete {
    //     error!("Got new data for completed trace");
    //     return Ok(());
    // }
    // debug!("Trying to start transaction");
    // let mut transaction = con
    //     .begin()
    //     .instrument(info_span!("start_transaction"))
    //     .await
    //     .map_err(|e| SqlxError::from_sqlx_error(e, "starting transaction"))?;
    // debug!("Started!");
    // if !trace_already_exists {
    //     insert_new_trace(
    //         &mut transaction,
    //         &instance_id,
    //         fragment.root_span.id,
    //         &fragment.root_span.name.clone(),
    //         fragment.root_span.timestamp,
    //     )
    //     .await?;
    //     insert_spans(
    //         &mut transaction,
    //         &vec![NewSpan {
    //             id: fragment.root_span.id,
    //             name: fragment.root_span.name.clone(),
    //             timestamp: fragment.root_span.timestamp,
    //             duration: fragment.root_span.duration,
    //             parent_id: None,
    //             key_vals: fragment.root_span.key_vals.clone(),
    //             location: fragment.root_span.location.clone(),
    //         }],
    //         fragment.root_span.id,
    //         instance_id,
    //     )
    //     .await?;
    // }
    // let open_spans = fragment.open_spans.clone();
    // let open_spans_ids: Vec<i64> = open_spans.keys().map(|e| (*e) as i64).collect();
    // let existing_span_id = get_existing_span_ids(
    //     &mut transaction,
    //     instance_id.instance_id,
    //     fragment.root_span.id as i64,
    //     &open_spans_ids,
    // )
    // .await?;
    // let mut spans_to_insert = HashMap::new();
    // for open_span in open_spans.values() {
    //     if !existing_span_id.contains(&(open_span.id as i64)) {
    //         if let Some(existing) = spans_to_insert.insert(
    //             open_span.id,
    //             NewSpan {
    //                 id: open_span.id,
    //                 name: open_span.name.clone(),
    //                 timestamp: open_span.timestamp,
    //                 duration: None,
    //                 parent_id: Some(open_span.parent_id),
    //                 key_vals: open_span.key_vals.clone(),
    //                 location: open_span.location.clone(),
    //             },
    //         ) {
    //             error!("Dup open span {existing:?}");
    //         }
    //     }
    // }
    // for s in &fragment.closed_spans {
    //     let new_span = NewSpan {
    //         id: s.id,
    //         name: s.name.clone(),
    //         timestamp: s.timestamp,
    //         duration: Some(s.duration),
    //         parent_id: Some(s.parent_id),
    //         key_vals: s.key_vals.clone(),
    //         location: s.location.clone(),
    //     };
    //     if let Some(existing) = spans_to_insert.insert(s.id, new_span.clone()) {
    //         error!("Dup closed span {existing:?}\n{new_span:?}");
    //     }
    // }
    // insert_spans(
    //     &mut transaction,
    //     &spans_to_insert
    //         .clone()
    //         .into_values()
    //         .collect::<Vec<NewSpan>>(),
    //     fragment.root_span.id,
    //     &instance_id,
    // )
    // .await?;
    // crate::api::database::insert_events(
    //     &mut transaction,
    //     &fragment.new_events,
    //     fragment.root_span.id,
    //     &instance_id,
    // )
    // .await?;
    // /*
    //     spans_produced
    //     events_produced
    //     events_dropped_by_sampling
    // */
    // let spans_produced = fragment.spans_produced as u64;
    // let events_produced = fragment.events_produced as u64;
    // let events_dropped_by_sampling = fragment.events_dropped_by_sampling as u64;
    // let stored_span_count_increase = {
    //     if trace_already_exists {
    //         spans_to_insert.len() as u64
    //     } else {
    //         spans_to_insert.len() as u64 + 1
    //     }
    // };
    // let stored_event_count_increase = fragment.new_events.len() as u64;
    // let size_bytes_increase = fragment.total_size();
    // let warnings_count_increase = fragment
    //     .new_events
    //     .iter()
    //     .filter(|e| matches!(e.level, api_structs::Severity::Warn))
    //     .count() as u64;
    // let has_errors = fragment
    //     .new_events
    //     .iter()
    //     .find(|e| matches!(e.level, api_structs::Severity::Error))
    //     .is_some();
    // debug!(
    //     "root_duration={:?}
    // spans_produced={spans_produced}
    // events_produced={events_produced}
    // events_dropped_by_sampling={events_dropped_by_sampling}
    // stored_span_count_increase={stored_span_count_increase}
    // stored_event_count_increase={stored_event_count_increase}
    // size_bytes_increase={size_bytes_increase}
    // warnings_count_increase={warnings_count_increase}
    // has_errors={has_errors}",
    //     fragment.root_span.duration
    // );
    // update_trace_header(
    //     &mut transaction,
    //     &instance_id,
    //     fragment.root_span.id,
    //     fragment.root_span.duration,
    //     spans_produced,
    //     stored_span_count_increase,
    //     events_produced,
    //     events_dropped_by_sampling,
    //     stored_event_count_increase,
    //     size_bytes_increase as u64,
    //     warnings_count_increase,
    //     has_errors,
    // )
    // .await?;
    // transaction.commit().await?;
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
    instance_id: &InstanceGlobalId,
    orphan_events: &[Event],
) {
    // info!("{} events to insert", orphan_events.len());
    // for e in orphan_events {
    //     trace!("Inserting event: {:?}", e);
    // }
    // let mut timestamps = vec![];
    // let mut envs = vec![];
    // let mut service_names = vec![];
    // let mut severities = vec![];
    // let mut message = vec![];
    // for event in orphan_events {
    //     timestamps.push(event.timestamp as i64);
    //     envs.push(instance_id.service_id.env.to_string());
    //     service_names.push(instance_id.service_id.name.as_str());
    //     severities.push(Severity::from(event.severity));
    //     message.push(event.message.clone());
    // }
    // let orphan_events_db_ids = match sqlx::query_scalar!(
    //         "insert into orphan_event (timestamp, env, service_name, severity, message) select * from unnest($1::BIGINT[], \
    //         $2::TEXT[], $3::TEXT[], $4::severity_level[], $5::TEXT[]) returning id;",
    //         &timestamps,
    //         &envs,
    //         &service_names as &Vec<&str>,
    //         severities.as_slice() as &[Severity],
    //         &message as &Vec<Option<String>>
    //     )
    //     .fetch_all(con).await {
    //     Ok(res) => {
    //         debug!("Inserted and got {} ids back", res.len());
    //         let res: Vec<i64> = res;
    //         res
    //     }
    //     Err(e) => {
    //         error!("Error inserting orphan events: {:#?}", e);
    //         error!("timestamp={:?}", timestamps);
    //         error!("service_names={:?}", service_names);
    //         error!("severities={:?}", severities);
    //         for v in message {
    //             error!("message={:?}", v);
    //         }
    //         return;
    //     }
    // };
    // let mut kv_orphan_event_id = vec![];
    // let mut kv_orphan_timestamp = vec![];
    // let mut kv_envs = vec![];
    // let mut kv_orphan_key = vec![];
    // let mut kv_orphan_value = vec![];
    // let mut kv_service_names = vec![];
    //
    // for (idx, event) in orphan_events.iter().enumerate() {
    //     for (key, val) in &event.key_vals {
    //         kv_orphan_event_id.push(orphan_events_db_ids[idx]);
    //         kv_orphan_timestamp.push(event.timestamp as i64);
    //         kv_envs.push(instance_id.service_id.env.to_string());
    //         kv_service_names.push(instance_id.service_id.name.as_str());
    //         kv_orphan_key.push(key.as_str());
    //         kv_orphan_value.push(val.as_str());
    //     }
    // }
    // info!("{} events key values to insert", kv_orphan_event_id.len());
    //
    // match sqlx::query!(
    //         "insert into orphan_event_key_value (orphan_event_id, key, value) select * from unnest($1::BIGINT[], $2::TEXT[], $3::TEXT[]);",
    //         &kv_orphan_event_id,
    //         &kv_orphan_key as &Vec<&str>,
    //         &kv_orphan_value as &Vec<&str>,
    //     )
    //     .execute(con).await {
    //     Ok(res) => {
    //         debug!("Inserted {}", res.rows_affected());
    //     }
    //     Err(e) => {
    //         error!("Error inserting orphan events: {:#?}", e);
    //         error!("kv_orphan_event_id={:?}", kv_orphan_event_id);
    //         error!("kv_orphan_key={:?}", kv_orphan_key);
    //         error!("kv_orphan_value={:?}", kv_orphan_value);
    //     }
    // };
    unimplemented!()
}

pub fn shorten_for_logging(text: &str, max_len: usize) -> String {
    // this is bytes, not chars, but close enough for debugging
    if text.len() > max_len {
        let first: String = text.chars().take(max_len / 2).collect();
        // we just got the chars in reverse order
        let last: String = text.chars().rev().take(max_len / 2).collect();
        let last = last.chars().rev().collect::<String>();
        format!("{first}\n...\n{last}")
    } else {
        text.to_string()
    }
}
#[instrument(level = "error", skip_all, err(Debug))]
pub async fn handler(
    State(app_state): State<AppState>,
    instance_snapshot: Json<InstanceSnapshot>,
) -> Result<Json<ConfigChange>, ApiError> {
    info!(
        instance_id=%instance_snapshot.instance_id,
        log_filter=instance_snapshot.log_filter,
        export_buffer_size_bytes=instance_snapshot.export_buffer_size_bytes,
        "got instance update"
    );

    let con = app_state.con;
    let mut tx = con.begin().await.map_err(SqlxError::from)?;
    let instance_id = instance_snapshot.instance_id;
    let instance_db_id = instance::database::get_instance_db_id(&mut tx, instance_id)
        .await?
        .ok_or_else(|| {
            error!(%instance_id, "got update for non existing instance");
            ApiError {
                code: StatusCode::BAD_REQUEST,
                message: "instance not registered".to_string(),
            }
        })?;
    info!(instance_db_id);
    let last_update_id =
        instance::database::get_last_instance_update_id(&mut tx, instance_db_id).await?;
    info!(last_update_id = last_update_id);
    let expected_value = match last_update_id {
        None => 0u64,
        Some(last_update_id) => (last_update_id + 1) as u64,
    };
    info!(expected_value);
    if instance_snapshot.id != expected_value {
        error!(
            received = instance_snapshot.id,
            expected = expected_value,
            last_update_id = last_update_id,
            "unexpected instance update id"
        );
        return Err(ApiError {
            code: StatusCode::BAD_REQUEST,
            message: format!(
                "unexpected instance update id, expected={expected_value} got {}",
                instance_snapshot.id
            ),
        });
    }
    let instance_update_id = instance_snapshot.id;
    instance::database::insert_instance_update(
        &mut tx,
        instance_db_id,
        instance_update_id,
        instance_snapshot.export_buffer_size_bytes,
    )
    .await?;
    for t in instance_snapshot.trace_snapshots.values() {
        trace::insert_or_update_trace(&mut tx, instance_db_id, instance_update_id, t).await?;
    }
    let cpu_profile_bytes = instance_snapshot
        .cpu_profile_base64
        .as_ref()
        .map(|profile| {
            base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(profile)
                .map_err(|e| {
                    let sample = shorten_for_logging(profile, 128);
                    error!(sample = sample, "Bad base64 profile");
                    ApiError {
                        code: StatusCode::BAD_REQUEST,
                        message: "Could not decode profile data as base64".to_string(),
                    }
                })
        })
        .transpose()?;

    instance::database::insert_instance_latest_log_filter_and_cpu_profile(
        &mut tx,
        instance_db_id,
        &instance_snapshot.log_filter,
        cpu_profile_bytes,
    )
    .await?;
    let config_change =
        instance::get_instance_config_change(&mut tx, instance_id, &instance_snapshot.log_filter)
            .await?;
    tx.commit().await.map_err(SqlxError::from)?;
    info!("instance update fully processed");
    Ok(config_change)
}

#[instrument(skip_all)]
async fn update_trace_header(
    con: &mut Transaction<'static, Postgres>,
    instance_id: &InstanceGlobalId,
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
    // sqlx::query!(
    //     "update trace
    //     set duration=$3,
    //         spans_produced=$4,
    //         spans_stored=($5 + spans_stored),
    //         events_produced=$6,
    //         events_dropped_by_sampling=$7,
    //         events_stored=($8 + events_stored),
    //         size_bytes=(size_bytes + $9),
    //         warnings=(warnings + $10),
    //         has_errors=(has_errors or $11)
    //     where instance_id = $1
    //       and id = $2;",
    //     instance_id.instance_id,                   //1
    //     trace_id as i64 as _,                      //2
    //     duration.map(|d| d as i64) as Option<i64>, //3
    //     spans_produced as i64 as _,                //4
    //     spans_stored_increase as i64 as _,         //5
    //     events_produced as i64 as _,               //6
    //     events_dropped_by_sampling as i64 as _,    //7
    //     stored_event_count_increase as i64 as _,   //8
    //     size_bytes_increase as i64 as _,           //9
    //     warnings_count_increase as i64 as _,       //10
    //     has_new_errors,                            //11
    // )
    // .execute(con.deref_mut())
    // .await
    // .map_err(|e| SqlxError::from_sqlx_error(e, "updating trace header"))?;
    // Ok(())
    unimplemented!()
}
