use api_structs::instance::update::Span;
use api_structs::time_conversion::time_from_nanos;
use api_structs::InstanceUpdateId;
use sqlx::{Postgres, Transaction};
use tracing::{info, info_span, instrument, Instrument};
use tracked_error::SqlxError;

pub type InstanceDbId = i32;
#[instrument(skip_all)]
pub async fn insert_spans_and_events(
    con: &mut Transaction<'static, Postgres>,
    instance_db_id: InstanceDbId,
    instance_update_id: InstanceUpdateId,
    trace_id: api_structs::TraceCountId,
    new_spans: impl Iterator<Item = &Span>,
) -> Result<(), SqlxError> {
    let mut instance_id_list = vec![];
    let mut trace_id_list = vec![];
    let mut span_id_list = vec![];
    let mut instance_update_id_list = vec![];
    let mut parent_id_list = vec![];
    let mut name_list = vec![];
    let mut created_at_list = vec![];
    let mut duration_nanos_list = vec![];
    let mut duration_is_final_list = vec![];
    let mut module_list = vec![];
    let mut filename_list = vec![];
    let mut line_list = vec![];
    // attributes
    let mut instance_id_attribute_list = vec![];
    let mut trace_id_attribute_list = vec![];
    let mut span_id_attribute_list = vec![];
    let mut name_attribute_list = vec![];
    let mut value_attribute_list = vec![];
    // events
    let mut instance_id_event_list = vec![];
    let mut trace_id_event_list = vec![];
    let mut span_id_event_list = vec![];
    let mut event_id_event_list = vec![];
    let mut instance_update_id_event_list = vec![];
    let mut logged_at_event_list = vec![];
    let mut message_event_list = vec![];
    let mut module_event_list = vec![];
    let mut filename_event_list = vec![];
    let mut line_event_list = vec![];
    let mut severity_event_list = vec![];
    // events attributes
    let mut instance_id_event_attribute_list = vec![];
    let mut trace_id_event_attribute_list = vec![];
    let mut span_id_event_attribute_list = vec![];
    let mut event_id_event_attribute_list = vec![];
    let mut name_event_attribute_list = vec![];
    let mut value_event_attribute_list = vec![];

    for s in new_spans {
        instance_id_list.push(instance_db_id);
        trace_id_list.push(trace_id as i32);
        span_id_list.push(s.id as i32);
        instance_update_id_list.push(instance_update_id as i32);
        parent_id_list.push(s.parent_id.map(|e| e as i32));
        name_list.push(s.name.clone());
        created_at_list.push(time_from_nanos(s.started_at_nanos));
        duration_nanos_list.push(s.duration_nanos as i64);
        duration_is_final_list.push(s.has_ended);
        module_list.push(s.location.module.clone());
        filename_list.push(s.location.filename.clone());
        line_list.push(s.location.line.map(|e| e as i32));
        for (attribute_name, json_value) in &s.attributes {
            instance_id_attribute_list.push(instance_db_id);
            trace_id_attribute_list.push(trace_id as i32);
            span_id_attribute_list.push(s.id as i32);
            name_attribute_list.push(attribute_name.clone());
            value_attribute_list.push(json_value.clone());
        }
        for (idx, event) in s.events.iter().enumerate() {
            instance_id_event_list.push(instance_db_id);
            trace_id_event_list.push(trace_id as i32);
            span_id_event_list.push(s.id as i32);
            event_id_event_list.push(idx as i32);
            instance_update_id_event_list.push(instance_update_id as i32);
            logged_at_event_list.push(time_from_nanos(event.timestamp));
            message_event_list.push(event.message.clone());
            module_event_list.push(event.location.module.clone());
            filename_event_list.push(event.location.filename.clone());
            line_event_list.push(event.location.line.map(|e| e as i32));
            severity_event_list.push(event.severity.to_string());
            for (key, value) in &event.attributes {
                instance_id_event_attribute_list.push(instance_db_id);
                trace_id_event_attribute_list.push(trace_id as i32);
                span_id_event_attribute_list.push(s.id as i32);
                event_id_event_attribute_list.push(idx as i32);
                name_event_attribute_list.push(key.clone());
                value_event_attribute_list.push(value.clone());
            }
        }
    }
    if span_id_list.is_empty() {
        info!("No spans to insert");
        return Ok(());
    } else {
        info!(span_count = span_id_list.len(), "Inserting spans");
    }
    sqlx::query!(
        "insert into span (instance_id,
                               trace_id,
                               span_id,
                               instance_update_id,
                               parent_id,
                               name,
                               created_at,
                               duration_nanos,
                               duration_is_final,
                               module,
                               filename,
                               line)
                select * from unnest(
                    $1::INT[],
                    $2::INT[],
                    $3::INT[],
                    $4::INT[],
                    $5::INT[],
                    $6::TEXT[],
                    $7::TIMESTAMP[],
                    $8::BIGINT[],
                    $9::BOOLEAN[],
                    $10::TEXT[],
                    $11::TEXT[],
                    $12::INT[]
                );",
        &instance_id_list,
        &trace_id_list,
        &span_id_list,
        &instance_update_id_list,
        &parent_id_list as &Vec<Option<i32>>,
        &name_list,
        &created_at_list,
        &duration_nanos_list,
        &duration_is_final_list,
        &module_list as &Vec<Option<String>>,
        &filename_list as &Vec<Option<String>>,
        &line_list as &Vec<Option<i32>>
    )
    .execute(&mut **con)
    .instrument(info_span!("span_insert"))
    .await?;
    sqlx::query!(
        "insert into span_attributes (instance_id,
                    trace_id,
                    span_id,
                    name,
                    value)
                select * from unnest(
                    $1::INT[],
                    $2::INT[],
                    $3::INT[],
                    $4::TEXT[],
                    $5::JSONB[]
                );",
        &instance_id_attribute_list,
        &trace_id_attribute_list,
        &span_id_attribute_list,
        &name_attribute_list,
        &value_attribute_list
    )
    .execute(&mut **con)
    .instrument(info_span!("span_attributes_insert"))
    .await?;
    sqlx::query!(
        "insert into event (instance_id, -- 1
                               trace_id, -- 2
                               span_id, -- 3
                               event_id, -- 4
                               instance_update_id, -- 5
                               logged_at, -- 6
                               message, -- 7
                               module, -- 8
                               filename, -- 9
                               line, -- 10
                               severity -- 11
                               )
                select * from unnest(
                    $1::INT[],
                    $2::INT[],
                    $3::INT[],
                    $4::INT[],
                    $5::INT[],
                    $6::TIMESTAMP[],
                    $7::TEXT[],
                    $8::TEXT[],
                    $9::TEXT[],
                    $10::INT[],
                    $11::TEXT[]
                );",
        &instance_id_event_list,
        &trace_id_event_list,
        &span_id_event_list,
        &event_id_event_list,
        &instance_update_id_event_list,
        &logged_at_event_list,
        &message_event_list as &Vec<Option<String>>,
        &module_event_list as &Vec<Option<String>>,
        &filename_event_list as &Vec<Option<String>>,
        &line_event_list as &Vec<Option<i32>>,
        &severity_event_list
    )
    .execute(&mut **con)
    .instrument(info_span!("event_insert"))
    .await?;
    sqlx::query!(
        "insert into event_attributes (instance_id, -- 1
                               trace_id, -- 2
                               span_id, -- 3
                               event_id, -- 4
                               name, -- 5
                               value -- 6
                               )
                select * from unnest(
                    $1::INT[],
                    $2::INT[],
                    $3::INT[],
                    $4::INT[],
                    $5::TEXT[],
                    $6::JSONB[]
                );",
        &instance_id_event_attribute_list,
        &trace_id_event_attribute_list,
        &span_id_event_attribute_list,
        &event_id_event_attribute_list,
        &name_event_attribute_list,
        &value_event_attribute_list
    )
    .execute(&mut **con)
    .instrument(info_span!("event_attribute_insert"))
    .await?;
    Ok(())
}
