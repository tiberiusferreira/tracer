use api_structs::InstanceId;
use chrono::NaiveDateTime;
use sqlx::{Postgres, Transaction};
use std::ops::DerefMut;
use tracing::{info, instrument};
use tracked_error::SqlxError;
use uuid::Uuid;

pub struct DbTrace {
    pub env: String,
    pub service_name: String,
    pub spans_produced: i32,
    pub events_produced: i32,
    pub events_dropped_by_sampling: i32,
}
#[instrument(skip_all)]
pub async fn insert_new_trace(
    con: &mut Transaction<'static, Postgres>,
    instance_id: &InstanceId,
    trace_id: i32,
    span_produced: i32,
    events_produced: i32,
    events_dropped_by_sampling: i32,
) -> Result<(), SqlxError> {
    // info!("Inserting trace header information for: {:?}", instance_id);
    // if let Err(e) = sqlx::query!(
    //     "insert into trace (env, service_name, instance_id, id, spans_produced, events_produced, events_dropped_by_sampling) values
    //     ($1, $2, $3, $4::int, $5::int, $6::int, $7::int);",
    //     instance_id.service_id.env.to_string() as _,
    //     instance_id.service_id.name as _,
    //     instance_id.instance_id as _,
    //     trace_id,
    //     span_produced,
    //     events_produced,
    //     events_dropped_by_sampling
    // )
    //     .execute(con.deref_mut())
    //     .await
    // {
    //     return Err(SqlxError::from_sqlx_error(e, "DB Error when inserting trace"));
    // }
    // Ok(())
    unimplemented!()
}

#[instrument(skip_all)]
pub async fn upsert_trace_cache(
    con: &mut Transaction<'static, Postgres>,
    instance_id: &InstanceId,
    trace_id: i32,
    timestamp: NaiveDateTime,
    top_level_span_name: &str,
    duration_nanos: i64,
    spans_stored_increment: i32,
    events_stored_increment: i32,
    size_bytes_increment: i32,
    warnings_increment: i32,
    has_errors: bool,
    closed: bool,
) -> Result<(), SqlxError> {
    //     info!("Upserting trace cache for: {:?}", instance_id);
    //     if let Err(e) = sqlx::query!(
    //         "insert into trace_cache (env,
    //                          service_name,
    //                          instance_id,
    //                          trace_id,
    //                          timestamp,
    //                          top_level_span_name,
    //                          duration_nanos,
    //                          spans_stored,
    //                          events_stored,
    //                          size_bytes,
    //                          warnings,
    //                          has_errors,
    //                          closed)
    // values ($1, $2, $3, $4::int, $5, $6::text, $7::bigint, $8::int, $9::int, $10::int, $11::int, $12, $13)
    // on conflict (env, service_name, instance_id, trace_id) do update
    //     set timestamp=$5,
    //         top_level_span_name=$6,
    //         duration_nanos=$7,
    //         spans_stored=excluded.spans_stored+$8,
    //         events_stored=excluded.events_stored+$9,
    //         size_bytes=excluded.size_bytes+$10,
    //         warnings=excluded.warnings+$11,
    //         has_errors=(excluded.has_errors or $12),
    //         closed=$13;",
    //         instance_id.service_id.env.to_string() as _,
    //         instance_id.service_id.name as _,
    //         instance_id.instance_id as _,
    //         trace_id,
    //         timestamp,
    //         top_level_span_name,
    //         duration_nanos,
    //         spans_stored_increment,
    //         events_stored_increment,
    //         size_bytes_increment,
    //         warnings_increment,
    //         has_errors,
    //         closed
    //     )
    //     .execute(con.deref_mut())
    //     .await
    //     {
    //         return Err(SqlxError::from_sqlx_error(
    //             e,
    //             "DB Error when upserting trace cache",
    //         ));
    //     }
    //     Ok(())
    unimplemented!()
}

#[instrument(skip_all)]
pub async fn update_trace_header(
    con: &mut Transaction<'static, Postgres>,
    instance_id: &InstanceId,
    trace_id: i32,
    span_produced: i32,
    events_produced: i32,
    events_dropped_by_sampling: i32,
) -> Result<(), SqlxError> {
    info!("Updating trace header information for: {:?}", instance_id);
    sqlx::query!(
        "update trace set spans_produced=$3::int, events_produced=$4::int, events_dropped_by_sampling=$5::int \
        where instance_id=$1 and id=$2;",
        instance_id.instance_id as _,
        trace_id,
        span_produced,
        events_produced,
        events_dropped_by_sampling
    )
    .execute(con.deref_mut())
    .await?;
    Ok(())
}

pub async fn get_trace_header(
    transaction: &mut Transaction<'static, Postgres>,
    instance_id: Uuid,
    trace_id: i32,
) -> Result<Option<DbTrace>, SqlxError> {
    //     sqlx::query_as!(
    //         DbTrace,
    //         "select env, service_name, spans_produced, events_produced, events_dropped_by_sampling
    // from trace
    // where instance_id = $1
    //   and id = $2;",
    //         instance_id,
    //         trace_id as i32
    //     )
    //     .fetch_optional(transaction.deref_mut())
    //     .await
    //     .map_err(|e| SqlxError::from_sqlx_error(e, "getting trace"))
    unimplemented!()
}
