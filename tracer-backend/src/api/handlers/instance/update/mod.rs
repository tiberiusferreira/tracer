use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::instance::update::{
    ConfigChange, ExecutionRecording, InstanceSnapshot, Parameter,
};
use axum::Json;
use axum::extract::State;
use gel_errors::{ErrorKind, UserError};
use gel_protocol::named_args;
use gel_tokio::{QueryExecutor, Queryable, RetryingTransaction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{info, instrument};
use tracing_config_helper::io_provider::Transaction;
use tracing_config_helper::io_provider::execution_recorder::record_single_attribute;
use tracked_error::TrackedError;
use uuid::Uuid;

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
    unimplemented!()
    //    let args = named_args! {
    //      "instance_id" => instance_snapshot.instance_id,
    //      "new_log_filter" => instance_snapshot.log_filter.as_str(),
    //    };
    //    let instance_update_data: Option<InstanceUpdatedData> = tx
    //        .query_single(
    //            r#"
    // with
    //  instance_id := <uuid>$instance_id,
    //  new_log_filter_value := <str>$new_log_filter,
    //  new_log_filter := (
    //      insert LogFilter {
    //        _value := new_log_filter_value
    //      } unless conflict on (._value)
    //      else
    //        (select LogFilter)
    //  ),
    //  service_instance := (
    //   update ServiceInstance filter .id=instance_id
    //   set {
    //     received_update_count := .received_update_count + 1,
    //     latest_log_filter := new_log_filter
    //   }
    // )
    // select {
    //   new_update_count := service_instance.received_update_count,
    //   desired_log_filter := service_instance.service.log_filter._value,
    // };
    //    "#,
    //            &args,
    //        )
    //        .await?;
    //    let instance_update_data = instance_update_data.ok_or(Error(TrackedError::from(
    //        ErrorVariants::InstanceNotRegistered,
    //    )))?;
    //    if instance_snapshot.update_count != instance_update_data.new_update_count as u64 {
    //        return Err(Error(TrackedError::from(
    //            ErrorVariants::UnexpectedUpdateCount {
    //                expected: instance_update_data.new_update_count,
    //                actual: instance_snapshot.update_count,
    //            },
    //        )))?;
    //    }
    //
    //    Ok(instance_update_data)
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

#[derive(Clone, Serialize, Deserialize)]
pub struct DbPartialExecution {
    pub id: Uuid,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub ended: bool,
}

async fn process_execution_recording(
    tx: &mut Transaction,
    instance_id: Uuid,
    recording: &ExecutionRecording,
) -> Result<(), api_structs::instance::update::Error> {
    let existing_execution: Option<DbPartialExecution> = tx
        .query_optional(
            "select Execution{
      id,
      last_seen_at,
      ended
    } filter
      .service_instance = <ServiceInstance><uuid>$service_instance_id
      and .external_id = <uuid>$external_id
      ",
            HashMap::from([
                (
                    "service_instance_id".to_string(),
                    Parameter::Uuid {
                        val: instance_id,
                        cast_to_table: None,
                    },
                ),
                (
                    "external_id".to_string(),
                    Parameter::Uuid {
                        val: recording.id,
                        cast_to_table: None,
                    },
                ),
            ]),
        )
        .await?;
    let execution_id = match existing_execution {
        None => {
            let params = HashMap::from([
                (
                    "service_instance",
                    Parameter::Uuid {
                        val: instance_id,
                        cast_to_table: Some("ServiceInstance".to_string()),
                    },
                ),
                (
                    "external_id",
                    Parameter::Uuid {
                        val: recording.id,
                        cast_to_table: None,
                    },
                ),
                ("started_at", Parameter::Datetime(recording.started_at)),
                ("last_seen_at", Parameter::Datetime(recording.last_seen_at)),
                ("ended", Parameter::Bool(recording.ended)),
                (
                    "replay_data",
                    Parameter::Json(serde_json::to_value(&recording.replay_data).unwrap()),
                ),
                (
                    "executed_functions",
                    Parameter::Json(serde_json::to_value(&recording.executed_functions).unwrap()),
                ),
            ]);
            let execution_id = tx.insert("Execution", params).await?;
            for (name, values) in &recording.attributes {
                for value in values {
                    let params = HashMap::from([
                        (
                            "execution",
                            Parameter::Uuid {
                                val: execution_id,
                                cast_to_table: Some("Execution".to_string()),
                            },
                        ),
                        ("name", Parameter::String(name.clone())),
                        ("_value", Parameter::String(value.clone())),
                    ]);
                    println!("inserted attribute");
                    let _id = tx.insert("Attributes", params).await?;
                }
            }
            execution_id
        }
        Some(existing_execution) => {
            // TODO update
            existing_execution.id
        }
    };
    Ok(())
}
async fn process_update(
    tx: &mut Transaction,
    instance_snapshot: &InstanceSnapshot,
) -> Result<(), api_structs::instance::update::Error> {
    for recording in &instance_snapshot.execution_recordings {
        // let is_from_self = recording
        //     .attributes
        //     .get("uri")
        //     .is_some_and(|uri| uri.contains("/api/instance/update"));
        // if is_from_self {
        //     let state = tx.state();
        //     tracing_config_helper::io_provider::execution_recorder::run_without_query_recording(
        //         &state,
        //         async {
        //             process_execution_recording(&mut *tx, instance_snapshot.instance_id, recording)
        //                 .await
        //         },
        //     )
        //     .await?;
        // } else {
        process_execution_recording(&mut *tx, instance_snapshot.instance_id, recording).await?;
        // }
    }
    Ok(())
}
#[instrument(level = "error", skip_all, err(Debug))]
pub async fn handler(
    State(app_state): State<AppState>,
    instance_snapshot: Json<InstanceSnapshot>,
) -> Result<(), ApiError> {
    let io_provider = app_state.execution_io_provider;

    let instance_snapshot = instance_snapshot.0;
    let db = io_provider.database().clone();
    let mut tx = db.transaction_start().await;
    process_update(&mut tx, &instance_snapshot).await?;
    tx.commit().await?;
    Ok(())
}
