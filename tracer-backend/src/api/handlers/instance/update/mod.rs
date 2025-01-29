use api_structs::instance::update::{ConfigChange, InstanceSnapshot};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use edgedb_codegen::edgedb_query;
use tracing::{error, info, info_span, instrument, Instrument};
use tracked_error::EdgeDBError;

use crate::api::state::AppState;
use crate::api::ApiError;

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

edgedb_query!(
    increase_instance_update_count_get_log,
    "
with service_instance := (
  update ServiceInstance filter .id=<uuid>$instance_id
  set {
    received_update_count := .received_update_count + 1
  }
)
select {
  received_update_count := service_instance.received_update_count,
  log_filter := service_instance.service.log_filter._value,
};
"
);

edgedb_query!(
    instance_update_insertion,
    "
with
instance_update := (
  insert ServiceInstanceUpdate {
    service_instance := (
      select ServiceInstance filter .id=<uuid>$instance_id
    ),
    export_buffer_size_bytes := <int64>$export_buffer_size_bytes
  }
)
select {
  instance_update_id := instance_update.id
};
"
);

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
    let edgedb_client = app_state.edgedb_client;
    let mut tx = edgedb_client
        .transaction()
        .instrument(info_span!("starting_transaction"))
        .await
        .map_err(|e| EdgeDBError::from(e))?;
    let increase_instance_update_count_get_log_res =
        increase_instance_update_count_get_log::transaction(
            &mut tx,
            &increase_instance_update_count_get_log::Input {
                instance_id: instance_snapshot.instance_id,
            },
        )
        .instrument(info_span!("increase_instance_update_count_get_log"))
        .await
        .map_err(|e| EdgeDBError::from(e))?;
    let (Some(received_update_count), Some(service_log_filter)) = (
        increase_instance_update_count_get_log_res.received_update_count,
        increase_instance_update_count_get_log_res.log_filter,
    ) else {
        error!(instance.id=%instance_snapshot.instance_id, "got update for non existing instance");
        return Err(ApiError {
            code: StatusCode::BAD_REQUEST,
            message: "instance not registered".to_string(),
        });
    };
    let expected_update_count = received_update_count as u64;

    info!(instance.expected_update_count = expected_update_count);
    if instance_snapshot.update_count != expected_update_count {
        error!(
            instance.received_update_count = instance_snapshot.update_count,
            instance.expected_update_count = expected_update_count,
            "unexpected instance update id"
        );
        return Err(ApiError {
            code: StatusCode::BAD_REQUEST,
            message: format!(
                "unexpected instance update id, expected={expected_update_count} got {}",
                instance_snapshot.update_count
            ),
        });
    }
    let instance_update_id = instance_update_insertion::transaction(
        &mut tx,
        &instance_update_insertion::Input {
            instance_id: instance_snapshot.instance_id,
            export_buffer_size_bytes: instance_snapshot.export_buffer_size_bytes as i64,
        },
    )
    .instrument(info_span!("insert_instance_update"))
    .await
    .map_err(|e| EdgeDBError::from(e))?
    .instance_update_id;
    let mut sorted_trace_fragments = instance_snapshot
        .trace_fragments
        .clone()
        .into_values()
        .collect::<Vec<_>>();
    sorted_trace_fragments.sort_by_key(|e| e.trace_count_id);

    for trace_fragment in &sorted_trace_fragments {
        trace::insert_or_update_trace(
            &mut tx,
            instance_snapshot.instance_id,
            instance_update_id,
            trace_fragment,
        )
        .await?;
    }

    tx.commit()
        .instrument(info_span!("commit_transaction"))
        .await
        .map_err(|e| EdgeDBError::from(e))?;
    let mut current: Vec<char> = instance_snapshot
        .log_filter
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let mut new: Vec<char> = service_log_filter
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    current.sort();
    new.sort();
    let log_filter_request = if current != new {
        info!(
            instance.log_filter = instance_snapshot.log_filter,
            service.log_filter = service_log_filter,
            "log filter change needed"
        );
        Some(service_log_filter)
    } else {
        None
    };
    info!("instance update fully processed");
    Ok(Json(ConfigChange {
        log_filter: log_filter_request,
    }))
}
