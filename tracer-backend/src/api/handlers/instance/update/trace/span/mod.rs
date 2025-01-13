use crate::api::handlers::instance::update::trace::TraceUpdateError;
use api_structs::instance::update::Span;
use edgedb_codegen::edgedb_query;
use std::collections::{HashMap, HashSet};
use tracing::{info, instrument};
use tracked_error::EdgeDBError;

#[derive(Debug, Clone, Hash)]
struct AttributeKeyValue {
    key: String,
    value: serde_json::Value,
}

#[instrument(skip_all)]
pub async fn insert_spans(
    tx: &mut edgedb_tokio::Transaction,
    instance_update_id: uuid::Uuid,
    trace_id: uuid::Uuid,
    spans_to_insert: &mut Vec<Span>,
) -> Result<(), TraceUpdateError> {
    info!(
        trace.id = trace_id.to_string(),
        instance.update.id = instance_update_id.to_string(),
        "inserting spans"
    );
    edgedb_query!(
        span_insertion,
        "with
  span_data := <json>$span_data,
  trace_id := <uuid>$trace_id,
  service_instance_update_id := <uuid>$service_instance_update_id,
for single_span in json_array_unpack(span_data) union (
  with
    attributes := json_object_unpack(single_span['attributes']),
    new_attributes := (
      for single_attribute in attributes union (
        insert Attribute{
          name := (
            insert AttributeName {
              name := single_attribute.0
            } unless conflict on .name else (select AttributeName)
          ),
          content := (
            insert AttributeContent {
              content := single_attribute.1
            } unless conflict on .content else (select AttributeContent)
          ),
        } unless conflict on (.name, .content) else (select Attribute)
      )
    ),
    name := (
      insert SpanName {
        name := <str>single_span['name']
      } unless conflict on .name else (select SpanName)
    ),
    insert Span  {
      trace := (select Trace filter .id=trace_id),
      instance_update := (select ServiceInstanceUpdate filter .id=service_instance_update_id),
      span_count_id := <int64>single_span['id'],
      has_ended := <bool>single_span['has_ended'],
      duration_nanos := <int64>single_span['duration_nanos'],
      started_at_nanos := <int64>single_span['started_at_nanos'],
      attributes := new_attributes,
      name := name,
      parent := (
        if exists <int64>single_span['parent_id'] then (
          assert_exists(
            assert_single(
                (
                  select detached Span
                  filter .span_count_id = <int64>single_span['parent_id']
                  and .trace.id = trace_id
                )
            )
          )
        )
        else {}
      )
    }
);"
    );
    edgedb_query!(event_insertion,
"with
  span_data := <json>$span_data,
  trace_id := <uuid>$trace_id,
  service_instance_update_id := <uuid>$service_instance_update_id,
for single_span in json_array_unpack(span_data) union (
  with
    span :=
      assert_exists(
            assert_single((
              select detached Span filter .trace.id = trace_id and .span_count_id=<int64>single_span['id'])
            )
      ),
  for single_event in json_array_unpack(single_span['events']) union (
    with
      event_attributes := json_object_unpack(single_event['attributes']),
      new_event_attributes := (
        for single_attribute in event_attributes union (
          insert Attribute{
            name := (
              insert AttributeName {
               name := single_attribute.0
              } unless conflict on .name else (select AttributeName)
            ),
            content := (
              insert AttributeContent {
                content := single_attribute.1
              } unless conflict on .content else (select AttributeContent)
              ),
          } unless conflict on (.name, .content) else (select Attribute)
        )
      ),
      message := (
        if exists <str>single_event['message'] then (
          insert EventMessage {
            message := <str>single_event['message']
          } unless conflict on .message else (select EventMessage)
        ) else {}
      ),
    timestamp := <int64>single_event['timestamp'],
    insert Event{
      attributes := new_event_attributes,
      timestamp := timestamp,
      message := message,
      span := span,
      service_instance_update := (select ServiceInstanceUpdate filter .id=service_instance_update_id),
    }
  )
);");
    spans_to_insert.sort_unstable_by_key(|e| e.id);
    for s in &*spans_to_insert {
        let spans_to_insert_json_str = serde_json::to_string(&vec![s.clone()]).unwrap();
        let res = span_insertion::transaction(
            &mut *tx,
            &span_insertion::Input {
                span_data: edgedb_protocol::model::Json::new_unchecked(spans_to_insert_json_str),
                trace_id,
                service_instance_update_id: instance_update_id,
            },
        )
        .await
        .map_err(|e| EdgeDBError::from(e))?;
        info!(inserted.span.span_count_id = s.id, "span inserted");
    }
    let spans_to_insert_json_str = serde_json::to_string(spans_to_insert).unwrap();
    let events = event_insertion::transaction(
        tx,
        &event_insertion::Input {
            span_data: edgedb_protocol::model::Json::new_unchecked(spans_to_insert_json_str),
            trace_id,
            service_instance_update_id: instance_update_id,
        },
    )
    .await
    .map_err(|e| EdgeDBError::from(e))?;
    info!(inserted.events.count = events.len(), "events inserted");

    Ok(())
}
