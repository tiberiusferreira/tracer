// // use crate::notification_worthy_events::{NotificationWorthyEventsPusher, TraceInvalidationCause};
// use crate::otel_trace_processing::span_processing::ValueType;
// use crate::proto_generated::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
// use crate::proto_generated::opentelemetry::proto::common::v1::KeyValue;
// use crate::proto_generated::opentelemetry::proto::trace::v1::{
//     ResourceSpans, Span as ProtoSpan, Span,
// };
//
// use crate::{EVENT_CHARS_LIMIT, MAX_COMBINED_SPAN_AND_EVENTS_PER_TRACE};
// use deepsize::DeepSizeOf;
// use futures::StreamExt;
// use sqlx::postgres::{PgHasArrayType, PgQueryResult, PgTypeInfo};
// use sqlx::{PgPool, Postgres, Transaction};
// use std::collections::{HashMap, HashSet};
// use std::ops::Deref;
// use std::sync::OnceLock;
// use std::time::{Duration, Instant};
// use tokio::task::JoinHandle;
// use tracing::{error, info, info_span, instrument, warn, Instrument};
//
// pub mod trace_fragment;
//
// pub mod span_processing;
//
// pub struct TraceStorage {
//     con: PgPool,
//     popper: trace_fragment::Popper,
//     notification_pusher: Option<NotificationWorthyEventsPusher>,
// }
//
// impl TraceStorage {
//     #[instrument(skip_all)]
//     pub fn initialize_and_start_storage_task(
//         con: PgPool,
//         time_between_runs: Duration,
//         notification_pusher: Option<NotificationWorthyEventsPusher>,
//     ) -> (trace_fragment::Pusher, JoinHandle<()>) {
//         static CELL: OnceLock<bool> = OnceLock::new();
//         let (incoming_traces_pusher, storer) = match CELL.set(true) {
//             Ok(()) => {
//                 let (pusher, popper) = trace_fragment::SharedBuffer::new();
//                 (
//                     pusher,
//                     Self {
//                         con,
//                         popper,
//                         notification_pusher,
//                     },
//                 )
//             }
//             Err(_e) => panic!("Tried to initialize otel_trace_processing::TraceStorage twice"),
//         };
//         info!("Starting otel_trace_processing::TraceStorage task");
//         let task_handle = tokio::task::spawn(async move {
//             loop {
//                 let traces = storer.popper.pop_ready_for_processing().await;
//                 if !traces.is_empty() {
//                     Self::validate_and_store_traces(
//                         &storer.con,
//                         traces,
//                         storer.notification_pusher.clone(),
//                     )
//                     .await;
//                 }
//                 tokio::time::sleep(time_between_runs).await;
//             }
//         });
//         (incoming_traces_pusher, task_handle)
//     }
//     #[instrument(skip_all)]
//     async fn validate_and_store_traces(
//         con: &PgPool,
//         traces: HashMap<ServiceName, HashMap<OtelTraceId, Vec<ProtoSpan>>>,
//         notification_pusher: Option<NotificationWorthyEventsPusher>,
//     ) {
//         let trace_processing_outcome =
//             Self::validate_traces_and_shape_for_db(traces, notification_pusher.clone()).await;
//         let inserted_traces = batch_store_traces(con, trace_processing_outcome).await;
//         if let Some(notification_pusher) = notification_pusher {
//             for trace in inserted_traces {
//                 notification_pusher
//                     .update_stats(
//                         trace.service_name.clone(),
//                         trace.top_level_span_name.clone(),
//                         trace.has_errors,
//                         trace.warning_count > 0,
//                         trace.span_plus_events_count,
//                     )
//                     .await;
//                 if trace.has_errors {
//                     notification_pusher
//                         .push_trace_with_error(trace.service_name.to_string(), trace)
//                         .await;
//                 }
//             }
//         }
//     }
//     #[instrument(skip_all)]
//     async fn validate_traces_and_shape_for_db(
//         traces: HashMap<ServiceName, HashMap<OtelTraceId, Vec<ProtoSpan>>>,
//         notification_pusher: Option<NotificationWorthyEventsPusher>,
//     ) -> Vec<DbReadyTraceData> {
//         let mut db_ready_trace_data = vec![];
//         for (service_name, traces) in traces {
//             for (_id, spans) in traces {
//                 match process_trace_data_for_insertion(service_name.to_string(), spans) {
//                     Ok(valid_data) => db_ready_trace_data.push(valid_data),
//                     Err(e) => {
//                         if let Some(notification_pusher) = &notification_pusher {
//                             notification_pusher
//                                 .push_invalid_traces(service_name.to_string(), e)
//                                 .await;
//                         }
//                     }
//                 };
//             }
//         }
//         db_ready_trace_data
//     }
// }
//
// fn group_spans_by_trace_id(spans: Vec<ProtoSpan>) -> HashMap<String, Vec<ProtoSpan>> {
//     spans.into_iter().fold(HashMap::new(), |mut acc, curr| {
//         let trace_id: String = base16::encode_lower(&curr.trace_id);
//         let entry: &mut Vec<ProtoSpan> = acc.entry(trace_id).or_default();
//         entry.push(curr);
//         acc
//     })
// }
//
// fn estimate_size_bytes(spans: &[ProtoSpan]) -> usize {
//     spans
//         .iter()
//         .fold(0, |acc, curr| acc.saturating_add(curr.deep_size_of()))
// }
//
// struct SingleTrace {
//     service_name: String,
//     trace_to_spans: HashMap<String, Vec<ProtoSpan>>,
// }
// fn extract_service_name_and_spans(resource_spans: ResourceSpans) -> Result<SingleTrace, Error> {
//     let service_name = span_processing::service_name_from_resource(
//         &resource_spans.resource.ok_or(Error::Malformed(
//             "Trace had no ResourceSpans so we couldn't get the service name".to_string(),
//         ))?,
//     )
//     .ok_or(Error::Malformed(
//         "Trace's ResourceSpans did not contain the service name".to_string(),
//     ))?;
//     let spans = resource_spans
//         .scope_spans
//         .into_iter()
//         .fold(Vec::new(), |mut acc, curr| {
//             acc.extend_from_slice(&curr.spans);
//             acc
//         });
//     let trace_to_spans = group_spans_by_trace_id(spans);
//     Ok(SingleTrace {
//         service_name,
//         trace_to_spans,
//     })
// }
//
// #[instrument(skip_all)]
// fn group_spans_by_service_and_trace_id(resource_spans: Vec<ResourceSpans>) -> OtelServiceTraces {
//     let mut new_service_traces: OtelServiceTraces = HashMap::new();
//     for r in resource_spans {
//         match extract_service_name_and_spans(r) {
//             Ok(new_traces) => {
//                 let existing_service_traces = new_service_traces
//                     .entry(new_traces.service_name.clone())
//                     .or_default();
//                 for (new_trace_id, spans) in new_traces.trace_to_spans {
//                     let existing_spans = existing_service_traces.entry(new_trace_id).or_default();
//                     existing_spans.extend_from_slice(spans.as_slice());
//                 }
//             }
//             Err(e) => {
//                 error!("{:?}", e);
//                 continue;
//             }
//         }
//     }
//     new_service_traces
// }
//
// #[derive(Debug, Clone)]
// pub enum Error {
//     Db(String),
//     Malformed(String),
// }
//
// impl From<sqlx::Error> for Error {
//     fn from(value: sqlx::Error) -> Self {
//         Error::Db(value.to_string())
//     }
// }
//
// // #[instrument(skip_all)]
// // async fn insert_trace_metadata(
// //     con: &mut Transaction<'static, Postgres>,
// //     service: &DbReadyTraceData,
// // ) -> Result<i64, Error> {
// //     let id = sqlx::query_scalar!(
// //         "insert into trace (timestamp, service_name, top_level_span_name, duration, warning_count, has_errors)
// //     values ($1::ubigint, $2, $3, $4, $5, $6) returning id;",
// //         service.timestamp as _,
// //         service.service_name as _,
// //         service.top_level_span_name as _,
// //         service.duration as _,
// //         i64::from(service.warning_count) as _,
// //         service.has_errors
// //     )
// //     .fetch_one(con)
// //     .await?;
// //     Ok(id)
// // }
//
// struct SpanIdToDbId(HashMap<Vec<u8>, i64>);
// impl Deref for SpanIdToDbId {
//     type Target = HashMap<Vec<u8>, i64>;
//
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }
//
// // #[instrument(skip_all)]
// // async fn insert_spans(
// //     con: &mut Transaction<'static, Postgres>,
// //     trace_id: i64,
// //     db_trace: &DbReadyTraceData,
// // ) -> Result<(), Error> {
// //     let spans = &db_trace.spans;
// //     let ids: Vec<i64> = (1..=spans.len())
// //         .map(|e| i64::try_from(e).expect("usize to fit i64"))
// //         .collect();
// //     let names: Vec<String> = spans.iter().map(|s| s.name.to_string()).collect();
// //     let timestamps: Vec<i64> = spans.iter().map(|s| s.timestamp).collect();
// //     let parent_ids: Vec<Option<i64>> = spans.iter().map(|s| s.parent_id).collect();
// //     let durations_ns: Vec<i64> = spans.iter().map(|s| s.duration).collect();
// //
// //     sqlx::query!(
// //         "insert into span (trace_id, id, timestamp, parent_id, duration, name)
// //         select $1::BIGINT, * from unnest($2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::BIGINT[], $6::TEXT[]);",
// //         trace_id,
// //         &ids,
// //         &timestamps,
// //         &parent_ids: Vec<Option<i64>>,
// //         &durations_ns,
// //         &names
// //     )
// //     .execute(&mut *con)
// //     .await?;
// //     Ok(())
// // }
//
// #[derive(Debug, Clone)]
// struct KeysToDbId(HashMap<String, i64>);
// impl Deref for KeysToDbId {
//     type Target = HashMap<String, i64>;
//
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }
//
// impl PgHasArrayType for Level {
//     fn array_type_info() -> PgTypeInfo {
//         PgTypeInfo::with_name("_severity_level")
//     }
// }
//
// impl PgHasArrayType for ValueType {
//     fn array_type_info() -> PgTypeInfo {
//         PgTypeInfo::with_name("_value_type")
//     }
// }
//
// fn key_is_user_generated(key: &str) -> bool {
//     static NON_USER_KEYS: [&str; 9] = [
//         "code.filepath",
//         "code.lineno",
//         "thread.id",
//         "code.namespace",
//         "thread.name",
//         "busy_ns",
//         "idle_ns",
//         "target",
//         "level",
//     ];
//     !NON_USER_KEYS.contains(&key)
// }
//
// // #[instrument(skip_all)]
// // async fn insert_span_keys(
// //     con: &mut Transaction<'static, Postgres>,
// //     trace_id: i64,
// //     db_trace: &DbReadyTraceData,
// // ) -> Result<(), Error> {
// //     let mut id: Vec<i64> = vec![];
// //     let mut timestamps: Vec<i64> = vec![];
// //     let mut key: Vec<String> = vec![];
// //     let mut user_generated: Vec<bool> = vec![];
// //     let mut value_type: Vec<ValueType> = vec![];
// //     let mut value: Vec<String> = vec![];
// //     for s in &db_trace.spans {
// //         for kv in &s.key_values {
// //             id.push(s.id);
// //             timestamps.push(s.timestamp);
// //             key.push(kv.key.clone());
// //             user_generated.push(key_is_user_generated(&kv.key));
// //             value_type.push(kv.value_type.clone());
// //             value.push(kv.value.clone());
// //         }
// //     }
// //     sqlx::query!(
// //         "insert into span_key_value (trace_id, user_generated, span_id, key, value_type, value)
// //         select $1::BIGINT, * from unnest($2::BOOLEAN[], $3::BIGINT[], $4::TEXT[], $5::value_type[], $6::TEXT[]);",
// //         trace_id,
// //         &user_generated,
// //         &id,
// //         &key,
// //         value_type.as_slice() as &[ValueType],
// //         &value
// //     )
// //     .execute(&mut *con)
// //     .instrument(info_span!("Inserting span keys"))
// //     .await?;
// //     Ok(())
// // }
//
// // #[instrument(skip_all)]
// // async fn insert_event_keys(
// //     con: &mut Transaction<'static, Postgres>,
// //     trace_id: i64,
// //     db_trace: &DbReadyTraceData,
// // ) -> Result<(), Error> {
// //     let mut span_ids: Vec<i64> = vec![];
// //     let mut event_id: Vec<i64> = vec![];
// //     let mut key: Vec<String> = vec![];
// //     let mut user_generated: Vec<bool> = vec![];
// //     let mut value_type: Vec<ValueType> = vec![];
// //     let mut value: Vec<String> = vec![];
// //     for s in &db_trace.spans {
// //         for e in &s.events {
// //             if e.name.is_empty() {
// //                 warn!("Dropping empty event Key Values: {:#?}", e);
// //                 continue;
// //             }
// //             for kv in &e.key_values {
// //                 span_ids.push(s.id);
// //                 event_id.push(e.id);
// //                 user_generated.push(key_is_user_generated(&kv.key));
// //                 key.push(kv.key.clone());
// //                 value_type.push(kv.value_type.clone());
// //                 value.push(kv.value.clone());
// //             }
// //         }
// //     }
// //     sqlx::query!(
// //         "insert into event_key_value (trace_id, user_generated, span_id, event_id, key, value_type, value)
// //         select $1::BIGINT, * from unnest($2::BOOLEAN[], $3::BIGINT[], $4::BIGINT[], $5::TEXT[], $6::value_type[], $7::TEXT[]);",
// //         trace_id,
// //         &user_generated,
// //         &span_ids,
// //         &event_id,
// //         &key,
// //         value_type.as_slice() as &[ValueType],
// //         &value
// //     )
// //     .execute(&mut *con)
// //     .instrument(info_span!("Inserting event keys"))
// //     .await?;
// //     Ok(())
// // }
//
// // #[instrument(skip_all)]
// // async fn insert_events(
// //     con: &mut Transaction<'static, Postgres>,
// //     trace_id: i64,
// //     db_trace: &DbReadyTraceData,
// // ) -> Result<(), Error> {
// //     let mut spans_id: Vec<i64> = vec![];
// //     let mut event_ids: Vec<i64> = vec![];
// //     let mut timestamp: Vec<i64> = vec![];
// //     let mut name: Vec<String> = vec![];
// //     let mut severity: Vec<Level> = vec![];
// //     for s in &db_trace.spans {
// //         for e in &s.events {
// //             if e.name.is_empty() {
// //                 warn!("Dropping empty event: {:#?}", e);
// //                 continue;
// //             }
// //             event_ids.push(e.id);
// //             spans_id.push(s.id);
// //             timestamp.push(e.timestamp);
// //             name.push(e.name.clone());
// //             severity.push(e.severity.clone());
// //         }
// //     }
// //     sqlx::query!(
// //         "insert into event (trace_id, span_id, id,
// //         timestamp, name, severity)
// //         select $1::BIGINT, * from unnest($2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::TEXT[], $6::severity_level[]);",
// //         trace_id,
// //         &spans_id,
// //         &event_ids,
// //         &timestamp,
// //         &name,
// //         &severity.as_slice() as &[Level]
// //     )
// //     .execute(&mut *con)
// //     .instrument(info_span!("Inserting events"))
// //     .await?;
// //     Ok(())
// // }
//
// // #[instrument(skip_all)]
// // async fn insert_trace_span_and_events(
// //     con: &mut Transaction<'static, Postgres>,
// //     trace_id: i64,
// //     db_trace: &DbReadyTraceData,
// // ) -> Result<(), Error> {
// //     insert_spans(con, trace_id, db_trace).await?;
// //     insert_span_keys(con, trace_id, db_trace).await?;
// //     insert_events(con, trace_id, db_trace).await?;
// //     insert_event_keys(con, trace_id, db_trace).await?;
// //     Ok(())
// // }
//

// #[instrument(skip_all)]
// pub async fn insert_all_trace_data(
//     trans: &mut Transaction<'static, Postgres>,
//     trace: &DbReadyTraceData,
// ) -> Result<i64, Error> {
//     let id = insert_trace_metadata(&mut *trans, trace).await?;
//     info!(
//         "Trace Metadata (id={id}) inserted for {} - {}",
//         trace.service_name, trace.top_level_span_name
//     );
//     insert_trace_span_and_events(&mut *trans, id, trace).await?;
//     info!("Inserted data for {}", trace.service_name);
//     Ok(id)
// }
//
// #[instrument(skip_all)]
// fn valid_data(spans: &Vec<ProtoSpan>) -> bool {
//     info!("Validating span data");
//     // spans must either root (no) parent or have a valid parent
//     let span_ids: HashSet<Vec<u8>> = spans.iter().map(|s| s.span_id.clone()).collect();
//     for s in spans {
//         if !s.parent_span_id.is_empty() {
//             let referenced_span = span_ids.get(&s.parent_span_id);
//             if referenced_span.is_none() {
//                 warn!("Span with invalid reference found");
//                 return false;
//             }
//         }
//     }
//     info!("Span has valid data");
//     true
// }
//
// // #[instrument(skip_all)]
// // pub async fn delete_old_traces(con: &PgPool) -> Result<(), Error> {
// //     let res: PgQueryResult =
// //         sqlx::query!("delete from trace where timestamp < (EXTRACT(epoch FROM now() - INTERVAL '1 DAY') * 1000000000);")
// //             .execute(con)
// //             .instrument(info_span!("deleting_old_traces"))
// //             .await?;
// //     info!("Deleted {} records", res.rows_affected());
// //     Ok(())
// // }
//
// // #[instrument(skip_all)]
// // pub async fn delete_old_traces_logging_errors(con: &PgPool) {
// //     if let Err(e) = delete_old_traces(con).await {
// //         error!("Error deleting old traces: {:#?}", e);
// //     }
// // }
//
// // #[instrument(skip_all)]
// // pub fn start_background_delete_traces_task(
// //     con: PgPool,
// //     time_between_runs: Duration,
// // ) -> JoinHandle<()> {
// //     tokio::spawn(async move {
// //         let con = con.clone();
// //         loop {
// //             delete_old_traces_logging_errors(&con).await;
// //             tokio::time::sleep(time_between_runs).await;
// //         }
// //     })
// // }
//
// // #[instrument(skip_all)]
// // async fn batch_store_traces(con: &PgPool, traces: Vec<DbReadyTraceData>) -> Vec<InsertedTrace> {
// //     info!(
// //         traces_to_store_cnt = traces.len(),
// //         "Going to store new traces"
// //     );
// //     let mut futs = vec![];
// //     let mut inserted_traces = vec![];
// //     for trace in traces {
// //         futs.push(async {
// //             let service_name = trace.service_name.to_string();
// //             let top_level_span_name = trace.top_level_span_name.to_string();
// //             let has_errors = trace.has_errors;
// //             let warning_count = trace.warning_count;
// //             let span_plus_events_count = trace.span_plus_events_count;
// //             let id = store_trace(con.clone(), trace).await?;
// //             Ok(InsertedTrace {
// //                 id,
// //                 service_name,
// //                 top_level_span_name,
// //                 has_errors,
// //                 warning_count,
// //                 span_plus_events_count,
// //             })
// //         });
// //     }
// //     let mut buffer = futures::stream::iter(futs).buffer_unordered(30);
// //     while let Some(res) = buffer.next().await {
// //         let res: Result<InsertedTrace, Error> = res;
// //         match res {
// //             Ok(inserted) => {
// //                 inserted_traces.push(inserted);
// //             }
// //             Err(err) => {
// //                 error!("Error storing trace: {:#?}", err);
// //             }
// //         }
// //     }
// //     inserted_traces
// // }
// //
// // #[derive(Debug, Clone)]
// // pub struct InsertedTrace {
// //     pub id: i64,
// //     pub service_name: String,
// //     pub top_level_span_name: String,
// //     pub has_errors: bool,
// //     pub warning_count: u32,
// //     span_plus_events_count: usize,
// // }
//
// #[derive(Debug, Clone, PartialEq, Eq, Hash)]
// pub struct ServiceTrace {
//     service_name: String,
//     trace_id: String,
// }
//
// pub type ServiceName = String;
// pub type OtelTraceId = String;
// pub type OtelServiceTraces = HashMap<ServiceName, HashMap<OtelTraceId, Vec<ProtoSpan>>>;
//
// #[derive(Debug, Clone)]
// pub struct PendingData {
//     first_data_received_at: Instant,
//     last_data_received_at: Instant,
//     dropped_over_size_limit: bool,
//     spans: Vec<ProtoSpan>,
// }
//
// #[instrument(skip_all)]
// pub async fn stage_trace_fragment(
//     request: ExportTraceServiceRequest,
//     trace_fragment_pusher: &trace_fragment::Pusher,
// ) {
//     let otel_service_traces = group_spans_by_service_and_trace_id(request.resource_spans);
//     trace_fragment_pusher.try_push(otel_service_traces).await;
// }
//
// pub struct DbReadyTraceData {
//     pub timestamp: i64,
//     pub service_name: String,
//     pub duration: i64,
//     pub top_level_span_name: String,
//     pub has_errors: bool,
//     pub warning_count: u32,
//     pub spans: Vec<DbSpan>,
//     pub span_plus_events_count: usize,
// }
// #[derive(Debug)]
// pub struct DbSpan {
//     pub id: i64,
//     pub timestamp: i64,
//     pub parent_id: Option<i64>,
//     pub name: String,
//     pub duration: i64,
//     pub key_values: Vec<DbKeyValue>,
//     pub events: Vec<DbEvent>,
// }
// #[derive(Debug)]
// pub struct DbEvent {
//     pub id: i64,
//     pub timestamp: i64,
//     pub name: String,
//     pub key_values: Vec<DbKeyValue>,
//     pub severity: Level,
// }
// #[derive(Debug)]
// pub struct DbKeyValue {
//     key: String,
//     value_type: ValueType,
//     value: String,
// }
//
// fn attributes_to_db(kvs: &[KeyValue]) -> Result<Vec<DbKeyValue>, TraceInvalidationCause> {
//     let key_values: Result<Vec<DbKeyValue>, TraceInvalidationCause> = kvs
//         .iter()
//         .map(|kv| {
//             let kv = span_processing::proto_key_value_to_supported(kv)?;
//             Ok(DbKeyValue {
//                 key: kv.key,
//                 value_type: kv.value.value_type,
//                 value: kv.value.value,
//             })
//         })
//         .collect();
//     key_values
// }
//
// fn trace_dropped_data_check(spans: &[Span]) -> Result<(), TraceInvalidationCause> {
//     let mut dropped_attributes_count = 0u32;
//     let mut dropped_links_count = 0u32;
//     let mut dropped_events_count = 0u32;
//     for s in spans {
//         dropped_attributes_count =
//             dropped_attributes_count.saturating_add(s.dropped_attributes_count);
//         dropped_links_count = dropped_links_count.saturating_add(s.dropped_links_count);
//         dropped_events_count = dropped_events_count.saturating_add(s.dropped_events_count);
//     }
//     let mut errs = vec![];
//     if dropped_attributes_count > 0 {
//         errs.push(format!("{} dropped attributes", dropped_attributes_count));
//     }
//     if dropped_links_count > 0 {
//         errs.push(format!("{} dropped links", dropped_links_count));
//     }
//     if dropped_events_count > 0 {
//         errs.push(format!("{} dropped events", dropped_events_count));
//     }
//     if errs.is_empty() {
//         Ok(())
//     } else {
//         Err(TraceInvalidationCause::from_cause(errs.join(". ").as_str()))
//     }
// }
//
// fn span_start_i64(span: &ProtoSpan) -> Result<i64, TraceInvalidationCause> {
//     i64::try_from(span.start_time_unix_nano)
//         .map_err(|_e| TraceInvalidationCause::from_cause("Span start did not fit i64"))
// }
// fn span_end_i64(span: &ProtoSpan) -> Result<i64, TraceInvalidationCause> {
//     i64::try_from(span.end_time_unix_nano)
//         .map_err(|_e| TraceInvalidationCause::from_cause("Span end did not fit i64"))
// }
// fn span_duration_i64(span: &ProtoSpan) -> Result<i64, TraceInvalidationCause> {
//     span_end_i64(span)?
//         .checked_sub(span_start_i64(span)?)
//         .ok_or(TraceInvalidationCause::from_cause(
//             "Span duration did not fit i64",
//         ))
// }
//
// #[instrument(skip_all)]
// fn process_trace_data_for_insertion(
//     service_name: String,
//     mut spans: Vec<Span>,
// ) -> Result<DbReadyTraceData, TraceInvalidationCause> {
//     trace_dropped_data_check(&spans)?;
//     let span_plus_events_count = spans.iter().fold(0, |mut acc: usize, curr| {
//         // separate 1 from span
//         acc = acc.saturating_add(1usize.saturating_add(curr.events.len()));
//         acc
//     });
//     if span_plus_events_count > MAX_COMBINED_SPAN_AND_EVENTS_PER_TRACE {
//         return Err(TraceInvalidationCause::from_cause(
//             format!(
//                 "More span+events than maximum allowed: {span_plus_events_count} vs {MAX_COMBINED_SPAN_AND_EVENTS_PER_TRACE}"
//             )
//             .as_str(),
//         ));
//     }
//     spans.sort_by_key(|s| s.start_time_unix_nano);
//     let has_errors = spans.iter().any(span_processing::has_errors);
//     let root_span = spans
//         .iter()
//         .find(|s| s.parent_span_id.is_empty())
//         .ok_or(TraceInvalidationCause::from_cause("No root span"))?;
//
//     let trace_start = span_start_i64(root_span)?;
//     let trace_duration = span_duration_i64(root_span)?;
//     let spans_otel_id_to_db_id =
//         spans
//             .iter()
//             .enumerate()
//             .fold(HashMap::new(), |mut acc, (idx, curr)| {
//                 acc.insert(
//                     curr.span_id.clone(),
//                     i64::try_from(idx + 1)
//                         .expect("usize to fit i64 since we have a limit on span count"),
//                 );
//                 acc
//             });
//     let mut db_spans: Vec<DbSpan> = vec![];
//     let mut warning_count: u32 = 0;
//     for s in &spans {
//         let self_db_id =
//             spans_otel_id_to_db_id
//                 .get(&s.span_id)
//                 .ok_or(TraceInvalidationCause::from_cause(
//                     "Bug in span id assignment",
//                 ))?;
//         if s.name.is_empty() {
//             return Err(TraceInvalidationCause::from_cause("Empty span name"));
//         }
//         let span_start = span_start_i64(s)?;
//         let span_duration = span_duration_i64(s)?;
//         let key_values: Vec<DbKeyValue> = attributes_to_db(&s.attributes)?;
//
//         let events: Result<Vec<DbEvent>, TraceInvalidationCause> = s
//             .events
//             .iter()
//             .enumerate()
//             .map(|(idx, e)| {
//                 if e.name.is_empty() {
//                     return Err(TraceInvalidationCause::from_cause(
//                         "Empty event, probably from #[instrument(err)]",
//                     ));
//                 }
//                 if e.name.len() > EVENT_CHARS_LIMIT {
//                     return Err(TraceInvalidationCause::from_cause(
//                         format!(
//                             "Event with more than {EVENT_CHARS_LIMIT} chars, had: {}",
//                             e.name.len()
//                         )
//                         .as_str(),
//                     ));
//                 }
//                 let event_timestamp = i64::try_from(e.time_unix_nano).map_err(|_e| {
//                     TraceInvalidationCause::from_cause("Event timestamp did not fit i64")
//                 })?;
//                 let key_values: Vec<DbKeyValue> = attributes_to_db(&e.attributes)?;
//                 let level = key_values
//                     .iter()
//                     .find(|kv| kv.key.as_str() == "level")
//                     .ok_or(TraceInvalidationCause::from_cause("Event had no level"))?;
//                 let level =
//                     Level::try_from(level.value.to_ascii_lowercase().as_str()).map_err(|_| {
//                         TraceInvalidationCause::from_cause(
//                             format!("Invalid event level: {}", level.value).as_str(),
//                         )
//                     })?;
//                 if let Level::Warn = level {
//                     warning_count = warning_count.saturating_add(1);
//                 }
//                 Ok(DbEvent {
//                     id: i64::try_from(idx + 1).expect("usize to fit i64"),
//                     timestamp: event_timestamp,
//                     name: e.name.to_string(),
//                     key_values,
//                     severity: level,
//                 })
//             })
//             .collect();
//         let events = events?;
//         let parent_id = if s.parent_span_id.is_empty() {
//             None
//         } else {
//             let parent_id = *spans_otel_id_to_db_id.get(&s.parent_span_id).ok_or(
//                 TraceInvalidationCause::from_cause("Non root span missing parent"),
//             )?;
//             Some(parent_id)
//         };
//         db_spans.push(DbSpan {
//             id: *self_db_id,
//             timestamp: span_start,
//             parent_id,
//             name: s.name.to_string(),
//             duration: span_duration,
//             key_values,
//             events,
//         });
//     }
//     Ok(DbReadyTraceData {
//         timestamp: trace_start,
//         service_name,
//         duration: trace_duration,
//         top_level_span_name: root_span.name.to_string(),
//         has_errors,
//         warning_count,
//         spans: db_spans,
//         span_plus_events_count,
//     })
// }
//
// #[instrument(skip_all)]
// pub async fn store_trace(con: PgPool, data_for_insertion: DbReadyTraceData) -> Result<i64, Error> {
//     let mut trans = con
//         .begin()
//         .instrument(info_span!("Starting DB transaction"))
//         .await?;
//     let trace_id = insert_all_trace_data(&mut trans, &data_for_insertion).await?;
//     trans
//         .commit()
//         .instrument(info_span!("Committing to DB"))
//         .await?;
//     Ok(trace_id)
// }
