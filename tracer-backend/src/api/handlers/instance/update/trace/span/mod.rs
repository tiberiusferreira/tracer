use gel_protocol::named_args;
use tracing::{info, instrument};

// #[instrument(skip_all)]
// pub async fn insert_spans_and_events(
//     tx: &mut gel_tokio::RetryingTransaction,
//     trace_id: uuid::Uuid,
//     spans_to_insert: &mut Vec<Span>,
// ) -> Result<(), gel_tokio::Error> {
//     info!(
//         trace.id = trace_id.to_string(),
//         spans_to_insert.len = spans_to_insert.len(),
//         "about to insert spans"
//     );
//     spans_to_insert.sort_unstable_by_key(|e| e.id);
//     let spans_to_insert_json_str =
//         gel_protocol::model::Json::new_unchecked(serde_json::to_string(&spans_to_insert).unwrap());
//     let args = named_args! {
//         "span_data" => spans_to_insert_json_str.clone(),
//         "trace_id" => trace_id,
//     };
//     tx.execute(
//         r#"
// with
//   span_data := <json>$span_data,
//   trace_id := <uuid>$trace_id,
// for single_span in json_array_unpack(span_data) union (
//   with
//     attributes := json_object_unpack(single_span['attributes']),
//     new_attributes := (
//       for single_attribute in attributes union (
//         insert Attribute{
//           normalized_name := (
//             insert NormalizedAttributeName {
//               _value := single_attribute.0
//             } unless conflict on ._value else (select NormalizedAttributeName)
//           ),
//           normalized_content := (
//             insert NormalizedAttributeContent {
//               _value := single_attribute.1
//             } unless conflict on ._value else (select NormalizedAttributeContent)
//           ),
//         } unless conflict on (.normalized_name, .normalized_content) else (select Attribute)
//       )
//     ),
//     name := (
//       insert NormalizedSpanName {
//         _value := <str>single_span['name']
//       } unless conflict on ._value else (select NormalizedSpanName)
//     ),
//     insert Span  {
//       trace := (select Trace filter .id=trace_id),
//       span_count_id := <int64>single_span['id'],
//       has_ended := <bool>single_span['has_ended'],
//       duration_nanos := <int64>single_span['duration_nanos'],
//       started_at_nanos := <int64>single_span['started_at_nanos'],
//       attributes := new_attributes,
//       normalized_name := name,
//       parent := {} # we set it after all spans are inserted
//     }
// )
// "#,
//         &args,
//     )
//     .await?;
//     info!("spans inserted");
//
//     let args = named_args! {
//         "span_data" => spans_to_insert_json_str.clone(),
//         "trace_id" => trace_id,
//     };
//     tx.execute(
//         r#"
// with
//   span_data := <json>$span_data,
//   trace_id := <uuid>$trace_id
// for single_span in json_array_unpack(span_data) union (
//     update Span
//     filter .trace.id = trace_id and .span_count_id = <int64>single_span['id']
//     set {
//       parent := (
//         if exists <int64>single_span['parent_id'] then (
//           assert_exists(
//             assert_single(
//                 (
//                   select detached Span
//                   filter .span_count_id = <int64>single_span['parent_id']
//                   and .trace.id = trace_id
//                 )
//             )
//           )
//         )
//         else {}
//       )
//     }
// );
// "#,
//         &args,
//     )
//     .await?;
//
//     info!("about to insert events");
//     let args = named_args! {
//         "span_data" => spans_to_insert_json_str.clone(),
//         "trace_id" => trace_id,
//     };
//     tx.execute(
//         r#"
//     with
//   span_data := <json>$span_data,
//   trace_id := <uuid>$trace_id,
// for single_span in json_array_unpack(span_data) union (
//   with
//     span :=
//       assert_exists(
//             assert_single((
//               select detached Span filter .trace.id = trace_id and .span_count_id=<int64>single_span['id'])
//             )
//       ),
//   for single_event in json_array_unpack(single_span['events']) union (
//     with
//       event_attributes := json_object_unpack(single_event['attributes']),
//       new_event_attributes := (
//         for single_attribute in event_attributes union (
//           insert Attribute{
//             normalized_name := (
//               insert NormalizedAttributeName {
//                _value := single_attribute.0
//               } unless conflict on ._value else (select NormalizedAttributeName)
//             ),
//             normalized_content := (
//               insert NormalizedAttributeContent {
//                 _value := single_attribute.1
//               } unless conflict on ._value else (select NormalizedAttributeContent)
//               ),
//           } unless conflict on (.normalized_name, .normalized_content) else (select Attribute)
//         )
//       ),
//       message := (
//         if exists <str>single_event['message'] then (
//           insert NormalizedEventMessage {
//             _value := <str>single_event['message']
//           } unless conflict on ._value else (select NormalizedEventMessage)
//         ) else {}
//       ),
//     timestamp := <int64>single_event['timestamp'],
//     insert Event{
//       attributes := new_event_attributes,
//       severity := <Severity>str_title(<str>single_event['severity']),
//       timestamp := timestamp,
//       normalized_message := message,
//       span := span
//     }
//   )
// );
// "#,
//         &args,
//     )
//         .await?;
//     info!("events inserted");
//     Ok(())
// }
