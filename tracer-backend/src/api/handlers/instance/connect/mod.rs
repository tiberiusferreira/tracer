use crate::api::handlers::instance::connect::service_initialization::{Error, ServiceConfig};
use crate::api::state::AppState;
use crate::api::{state, ApiError, LiveServiceInstance};
use api_structs::{InstanceId, ServiceId};
use axum::extract::State;
use futures::StreamExt;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{info, instrument, trace, warn};
use tracked_error::SqlxError;

pub mod service_initialization;
#[derive(Debug, Clone)]
pub struct ChangeFilterInternalRequest {
    pub filters: String,
}

#[derive(Clone)]
pub struct LiveInstances {
    pub trace_data:
        std::sync::Arc<parking_lot::RwLock<HashMap<ServiceId, Vec<LiveServiceInstance>>>>,
    pub see_handle: std::sync::Arc<
        parking_lot::RwLock<
            HashMap<InstanceId, tokio::sync::mpsc::Sender<ChangeFilterInternalRequest>>,
        >,
    >,
}

#[instrument(skip_all)]
async fn change_filter_request(
    mut r: Receiver<ChangeFilterInternalRequest>,
) -> Option<(
    axum::response::sse::Event,
    Receiver<ChangeFilterInternalRequest>,
)> {
    info!("Waiting for new ChangeFilterInternalRequest");
    let request = match r.recv().await {
        None => {
            info!("Channel closed, closing sse channel.");
            return None;
        }
        Some(request) => request,
    };
    info!("new internal change filter request: {:?}", request);

    let data = api_structs::instance::connect::SseRequest::NewFilter {
        filter: request.filters,
    };
    let see = axum::response::sse::Event::default()
        .data(serde_json::to_string(&data).expect("to be serializable"));
    Some((see, r))
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SseError(String);

impl From<ApiError> for SseError {
    fn from(value: ApiError) -> Self {
        Self(value.message)
    }
}

#[instrument(skip_all)]
pub(crate) async fn handler(
    State(app_state): State<AppState>,
    instance_id: axum::extract::Query<InstanceId>,
) -> axum::response::Sse<
    std::pin::Pin<
        Box<
            dyn futures::stream::Stream<Item = Result<axum::response::sse::Event, SseError>> + Send,
        >,
    >,
> {
    let instance_id = instance_id.0;
    trace!("New SSE connection request for {:?}", instance_id);
    let mut transaction = app_state.con.begin().await.unwrap();
    let _service_config = match service_initialization::get_or_init_service_config(
        &mut transaction,
        &instance_id.service_id,
    )
    .await
    {
        Ok(config) => config,
        Err(e) => {
            let stream = Box::pin(futures::stream::once(async {
                Err(SseError::from(crate::api::ApiError::from(e)))
            }));
            return axum::response::sse::Sse::new(stream);
        }
    };
    let (see_handle, r) = tokio::sync::mpsc::channel(1);

    let mut w_lock = app_state.connected_instances_sse_handle.write();
    let w = w_lock.entry(instance_id.clone());
    match w {
        Entry::Occupied(mut existing) => {
            warn!(instance = ?instance_id, "replacing instance service sse handle");
            existing.insert(see_handle);
        }
        Entry::Vacant(vacant) => {
            vacant.insert(see_handle);
        }
    }

    let new_stream = Box::pin(futures::stream::unfold(r, |r| change_filter_request(r)).map(Ok));
    let new_stream = new_stream
        as std::pin::Pin<
            Box<
                dyn futures::stream::Stream<Item = Result<axum::response::sse::Event, SseError>>
                    + Send,
            >,
        >;
    axum::response::sse::Sse::new(new_stream).keep_alive(axum::response::sse::KeepAlive::default())
}
