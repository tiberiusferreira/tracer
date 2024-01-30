use crate::api::handlers::Severity;
use api_structs::instance::update::NewSpanEvent;
use api_structs::InstanceId;
use sqlx::{Postgres, Transaction};
use std::collections::HashSet;
use std::ops::DerefMut;
use tracing::{error, info, instrument, trace};

pub mod alerts;

#[instrument(skip_all)]
pub(crate) async fn insert_events(
    con: &mut Transaction<'static, Postgres>,
    new_events: &[NewSpanEvent],
    trace_id: u64,
    instance_id: &InstanceId,
    relocated_event_vec_indexes: &HashSet<usize>,
) -> Result<(), sqlx::Error> {
    if new_events.is_empty() {
        info!("No new trace events to insert");
        return Ok(());
    } else {
        info!("Inserting {} events", new_events.len());
    }
    for e in new_events {
        trace!("Inserting event: {:?}", e);
    }
    let instance_ids: Vec<i64> = new_events
        .iter()
        .map(|_s| instance_id.instance_id)
        .collect();
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
    let db_event_ids: Vec<i64> = match sqlx::query_scalar!(
            "insert into event (instance_id, trace_id, span_id, timestamp, message, severity, relocated)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::TEXT[], $6::severity_level[], $7::BOOLEAN[]) returning id;",
            &instance_ids,
            &trace_ids,
            &span_ids,
            &timestamps,
            &names as &Vec<Option<String>>,
            severities.as_slice() as &[Severity],
            &relocateds,
        )
        .fetch_all(con.deref_mut())
        .await{
        Ok(ids) => {
            info!(inserted_events_count=ids.len(),"Inserted events");
            ids
        }
        Err(e) => {
            error!("Error when inserting events");
            error!("service_ids={:?}", instance_ids);
            error!("trace_id={:?}", trace_id);
            error!("span_ids={:?}", span_ids);
            error!("timestamps={:?}", timestamps);
            for name in names{
                error!("name={:?}", name);
            }
            error!("severities={:?}", severities);
            error!("relocateds={:?}", relocateds);
            return Err(e);
        }
    };

    let mut kv_instance_id = vec![];
    let mut kv_trace_id = vec![];
    let mut kv_span_id = vec![];
    let mut kv_event_id = vec![];
    let mut kv_key = vec![];
    let mut kv_value = vec![];
    for (idx, span) in new_events.iter().enumerate() {
        for (key, val) in &span.key_vals {
            kv_instance_id.push(instance_id.instance_id);
            kv_trace_id.push(trace_id as i64);
            kv_span_id.push(span.span_id as i64);
            kv_event_id.push(db_event_ids[idx]);
            kv_key.push(key.as_str());
            kv_value.push(val.as_str());
        }
    }
    if kv_instance_id.is_empty() {
        info!("No event key-values to insert");
        return Ok(());
    } else {
        info!("Inserting {} event key-values", kv_instance_id.len());
    }
    match sqlx::query!(
            "insert into event_key_value (instance_id, trace_id, span_id, event_id, key, value)
            select * from unnest($1::BIGINT[], $2::BIGINT[], $3::BIGINT[], $4::BIGINT[], $5::TEXT[], $6::TEXT[]);",
            &kv_instance_id,
            &kv_trace_id,
            &kv_span_id,
            &kv_event_id,
            &kv_key as &Vec<&str>,
            &kv_value as &Vec<&str>,
        )
        .execute(con.deref_mut())
        .await {
        Ok(_) => {
            info!("Inserted span key-values");
        },
        Err(e) => {
            error!("Error when inserting span key-values");
            error!("kv_service_id: {:?}", kv_instance_id);
            error!("kv_trace_id: {:?}", kv_trace_id);
            error!("kv_span_id: {:?}", kv_span_id);
            error!("kv_key: {:?}", kv_key);
            error!("kv_value: {:?}", kv_value);
            return Err(e)
        }
    };

    Ok(())
}
