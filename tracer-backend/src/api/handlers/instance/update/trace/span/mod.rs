use crate::api::handlers::instance::update::trace::TraceUpdateError;
use api_structs::instance::update::Span;
use edgedb_codegen::edgedb_query;
use tracing::{info, instrument};
use tracked_error::EdgeDBError;
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
          normalized_name := (
            insert NormalizedAttributeName {
              _value := single_attribute.0
            } unless conflict on ._value else (select NormalizedAttributeName)
          ),
          normalized_content := (
            insert NormalizedAttributeContent {
              _value := single_attribute.1
            } unless conflict on ._value else (select NormalizedAttributeContent)
          ),
        } unless conflict on (.normalized_name, .normalized_content) else (select Attribute)
      )
    ),
    name := (
      insert NormalizedSpanName {
        _value := <str>single_span['name']
      } unless conflict on ._value else (select NormalizedSpanName)
    ),
    insert Span  {
      trace := (select Trace filter .id=trace_id),
      instance_update := (select ServiceInstanceUpdate filter .id=service_instance_update_id),
      span_count_id := <int64>single_span['id'],
      has_ended := <bool>single_span['has_ended'],
      duration_nanos := <int64>single_span['duration_nanos'],
      started_at_nanos := <int64>single_span['started_at_nanos'],
      attributes := new_attributes,
      normalized_name := name,
      parent := {} # we set it after all spans are inserted
    }
);"
);

edgedb_query!(
    set_span_parent,
    "
with
  span_data := <json>$span_data,
  trace_id := <uuid>$trace_id
for single_span in json_array_unpack(span_data) union (
    update Span
    filter .trace.id = trace_id and .span_count_id = <int64>single_span['id']
    set {
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
);
"
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
            normalized_name := (
              insert NormalizedAttributeName {
               _value := single_attribute.0
              } unless conflict on ._value else (select NormalizedAttributeName)
            ),
            normalized_content := (
              insert NormalizedAttributeContent {
                _value := single_attribute.1
              } unless conflict on ._value else (select NormalizedAttributeContent)
              ),
          } unless conflict on (.normalized_name, .normalized_content) else (select Attribute)
        )
      ),
      message := (
        if exists <str>single_event['message'] then (
          insert NormalizedEventMessage {
            _value := <str>single_event['message']
          } unless conflict on ._value else (select NormalizedEventMessage)
        ) else {}
      ),
    timestamp := <int64>single_event['timestamp'],
    insert Event{
      attributes := new_event_attributes,
      severity := <Severity>str_title(<str>single_event['severity']),
      timestamp := timestamp,
      normalized_message := message,
      span := span,
      service_instance_update := (select ServiceInstanceUpdate filter .id=service_instance_update_id),
    }
  )
);");

#[instrument(skip_all)]
pub async fn insert_spans_and_events(
    tx: &mut edgedb_tokio::Transaction,
    instance_update_id: uuid::Uuid,
    trace_id: uuid::Uuid,
    spans_to_insert: &mut Vec<Span>,
) -> Result<(), TraceUpdateError> {
    info!(
        trace.id = trace_id.to_string(),
        instance.update.id = instance_update_id.to_string(),
        spans_to_insert.len = spans_to_insert.len(),
        "about to insert spans"
    );
    spans_to_insert.sort_unstable_by_key(|e| e.id);
    let spans_to_insert_json_str = serde_json::to_string(&spans_to_insert).unwrap();
    let inserted_spans = span_insertion::transaction(
        &mut *tx,
        &span_insertion::Input {
            span_data: edgedb_protocol::model::Json::new_unchecked(
                spans_to_insert_json_str.clone(),
            ),
            trace_id,
            service_instance_update_id: instance_update_id,
        },
    )
    .await
    .map_err(|e| {
        info!(
            spans.insert.data = spans_to_insert_json_str,
            "raw spans insert json data"
        );
        EdgeDBError::from(e)
    })?;
    info!(
        inserted.spans.count = inserted_spans.len(),
        "spans inserted"
    );
    let _res = set_span_parent::transaction(
        &mut *tx,
        &set_span_parent::Input {
            span_data: edgedb_protocol::model::Json::new_unchecked(
                spans_to_insert_json_str.clone(),
            ),
            trace_id,
        },
    )
    .await
    .map_err(|e| {
        info!(
            spans.parent_set.data = spans_to_insert_json_str,
            "raw spans parent set json data"
        );
        EdgeDBError::from(e)
    })?;
    let inserted_events = event_insertion::transaction(
        tx,
        &event_insertion::Input {
            span_data: edgedb_protocol::model::Json::new_unchecked(
                spans_to_insert_json_str.clone(),
            ),
            trace_id,
            service_instance_update_id: instance_update_id,
        },
    )
    .await
    .map_err(|e| {
        info!(
            events.insert.data = spans_to_insert_json_str,
            "raw events insert json data"
        );
        EdgeDBError::from(e)
    })?;
    info!(
        inserted.events.count = inserted_events.len(),
        "events inserted"
    );

    Ok(())
}
