use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::instance::update::{ConfigChange, InstanceSnapshot};
use axum::Json;
use axum::extract::State;
use gel_errors::{ErrorKind, UserError};
use gel_protocol::named_args;
use gel_tokio::{QueryExecutor, Queryable, RetryingTransaction};
use thiserror::Error;
use tracing::{info, instrument};
use tracked_error::TrackedError;

mod trace;

#[allow(unused)]
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
//
// edgedb_query!(
//     increase_instance_update_count_get_log,
//     "
// with service_instance := (
//   update ServiceInstance filter .id=<uuid>$instance_id
//   set {
//     received_update_count := .received_update_count + 1
//   }
// )
// select {
//   received_update_count := service_instance.received_update_count,
//   log_filter := service_instance.service.log_filter._value,
// };
// "
// );
//
// edgedb_query!(
//     instance_update_insertion,
//     "
// with
// instance_update := (
//   insert ServiceInstanceUpdate {
//     service_instance := (
//       select ServiceInstance filter .id=<uuid>$instance_id
//     ),
//     export_buffer_size_bytes := <int64>$export_buffer_size_bytes
//   }
// )
// select {
//   instance_update_id := instance_update.id
// };
// "
// );

#[derive(Queryable)]
struct InstanceUpdatedData {
    new_update_count: i64,
    desired_log_filter: String,
}

#[derive(Error, Debug, Clone)]
enum ErrorVariants {
    #[error("Instance not registered")]
    InstanceNotRegistered,
    #[error("Unexpected update count. Expected: {expected}, actual: {actual}")]
    UnexpectedUpdateCount { expected: i64, actual: u64 },
}

#[derive(Error, Debug, Clone)]
#[error(transparent)]
struct Error(TrackedError<ErrorVariants>);

impl From<Error> for gel_tokio::Error {
    fn from(value: Error) -> Self {
        UserError::with_source(value)
    }
}
async fn update_instance_update_count_and_log_level(
    tx: &mut RetryingTransaction,
    instance_snapshot: &InstanceSnapshot,
) -> Result<InstanceUpdatedData, gel_tokio::Error> {
    let args = named_args! {
      "instance_id" => instance_snapshot.instance_id,
      "new_log_filter" => instance_snapshot.log_filter.as_str(),
    };
    let instance_update_data: Option<InstanceUpdatedData> = tx
        .query_single(
            r#"
 with
  instance_id := <uuid>$instance_id,
  new_log_filter_value := <str>$new_log_filter,
  new_log_filter := (
      insert LogFilter {
        _value := new_log_filter_value
      } unless conflict on (._value)
      else
        (select LogFilter)
  ),
  service_instance := (
   update ServiceInstance filter .id=instance_id
   set {
     received_update_count := .received_update_count + 1,
     latest_log_filter := new_log_filter
   }
 )
 select {
   new_update_count := service_instance.received_update_count,
   desired_log_filter := service_instance.service.log_filter._value,
 };
    "#,
            &args,
        )
        .await?;
    let instance_update_data = instance_update_data.ok_or(Error(TrackedError::from(
        ErrorVariants::InstanceNotRegistered,
    )))?;
    if instance_snapshot.update_count != instance_update_data.new_update_count as u64 {
        return Err(Error(TrackedError::from(
            ErrorVariants::UnexpectedUpdateCount {
                expected: instance_update_data.new_update_count,
                actual: instance_snapshot.update_count,
            },
        )))?;
    }

    Ok(instance_update_data)
}

async fn insert_new_instance_update(
    tx: &mut RetryingTransaction,
    instance_snapshot: &InstanceSnapshot,
) -> Result<(), gel_tokio::Error> {
    let args = named_args! {
      "instance_id" => instance_snapshot.instance_id,
      "export_buffer_size_bytes" => instance_snapshot.export_buffer_size_bytes as i64,
      "produced_at" => gel_protocol::model::Datetime::try_from(chrono::Utc::now()).expect("chrono date time to always be valid"),
    };
    tx.execute(
        r#"
 with
  instance_id := <uuid>$instance_id,
  export_buffer_size_bytes := <int64>$export_buffer_size_bytes,
  produced_at := <datetime>$produced_at,
  insert ServiceInstanceUpdate{
    export_buffer_size_bytes := export_buffer_size_bytes,
    produced_at := produced_at,
    service_instance := <ServiceInstance>instance_id
  }
    "#,
        &args,
    )
    .await?;
    Ok(())
}

async fn process_update(
    mut tx: RetryingTransaction,
    instance_snapshot: &InstanceSnapshot,
) -> Result<InstanceUpdatedData, gel_tokio::Error> {
    let updated_data =
        update_instance_update_count_and_log_level(&mut tx, instance_snapshot).await?;
    insert_new_instance_update(&mut tx, instance_snapshot).await?;
    let mut sorted_trace_fragments = instance_snapshot
        .trace_fragments
        .clone()
        .into_values()
        .collect::<Vec<_>>();
    sorted_trace_fragments.sort_by_key(|e| e.trace_count_id);

    for trace_fragment in &sorted_trace_fragments {
        trace::insert_or_update_trace(&mut tx, instance_snapshot.instance_id, trace_fragment)
            .await?;
    }
    Ok(updated_data)
}
#[instrument(level = "error", skip_all, err(Debug))]
pub async fn handler(
    State(app_state): State<AppState>,
    instance_snapshot: Json<InstanceSnapshot>,
) -> Result<Json<ConfigChange>, ApiError> {
    info!(
        instance.id=%instance_snapshot.instance_id,
        instance.log_filter=instance_snapshot.log_filter,
        instance.export_buffer_size_bytes=instance_snapshot.export_buffer_size_bytes,
        "got instance update"
    );
    let instance_snapshot = instance_snapshot.0;
    let gel_client = app_state.gel_client;
    let updated_data = gel_client
        .transaction(|tx| process_update(tx, &instance_snapshot))
        .await?;
    let mut current: Vec<char> = instance_snapshot
        .log_filter
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let mut new: Vec<char> = updated_data
        .desired_log_filter
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    current.sort();
    new.sort();
    let log_filter_request = if current != new {
        info!(
            instance.log_filter = instance_snapshot.log_filter,
            service.log_filter = updated_data.desired_log_filter,
            "log filter change needed"
        );
        Some(updated_data.desired_log_filter)
    } else {
        None
    };
    Ok(Json(ConfigChange {
        log_filter: log_filter_request,
    }))
}
