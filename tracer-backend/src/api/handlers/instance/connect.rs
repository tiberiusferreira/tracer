use crate::api::state::AppState;
use crate::api::{database, state, ApiError, LiveServiceInstance};
use api_structs::{InstanceId, ServiceId};
use axum::extract::State;
use futures::StreamExt;
use std::collections::{HashMap, VecDeque};
use tokio::sync::mpsc::Receiver;
use tracing::{info, instrument, trace};
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
pub(crate) async fn instance_connect_post(
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
    let exists = {
        let w_lock = app_state.instance_runtime_stats.read();
        w_lock.get(&instance_id.service_id).is_some()
    };
    if !exists {
        let config = match database::get_or_init_service_alert_config(
            &app_state.con,
            &instance_id.service_id.name,
            instance_id.service_id.env.clone(),
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
        let mut w_lock = app_state.instance_runtime_stats.write();
        w_lock.insert(
            instance_id.service_id.clone(),
            state::ServiceData {
                alert_config: config,
                instances: HashMap::new(),
            },
        );
    }
    let mut w_lock = app_state.instance_runtime_stats.write();
    let instance_list = &mut w_lock
        .get_mut(&instance_id.service_id)
        .expect("To exist, just inserted")
        .instances;
    let (see_handle, r) = tokio::sync::mpsc::channel(1);
    instance_list.insert(
        instance_id.instance_id,
        state::InstanceState {
            id: instance_id.instance_id,
            rust_log: "".to_string(),
            profile_data: None,
            time_data_points: VecDeque::new(),
            see_handle,
        },
    );
    drop(w_lock);
    let stream = Box::pin(futures::stream::unfold(r, |r| change_filter_request(r)).map(Ok));
    let stream = stream
        as std::pin::Pin<
            Box<
                dyn futures::stream::Stream<Item = Result<axum::response::sse::Event, SseError>>
                    + Send,
            >,
        >;
    axum::response::sse::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}
