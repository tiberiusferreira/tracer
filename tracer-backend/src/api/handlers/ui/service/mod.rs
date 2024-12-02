use crate::api::state::AppState;
use crate::api::{ApiError, AppStateError, ServiceInAppStateButNotDBError};
use api_structs::ui::service::Instance;
use api_structs::{Endpoint, ServiceId};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use tracing::instrument;
use tracked_error::{SerdeJsonError, SqlxError};

#[instrument(level = "error", skip_all, err(Debug))]
pub(crate) async fn ui_service_filter_post(
    State(app_state): State<AppState>,
    Json(new_filter): Json<api_structs::ui::service::NewFiltersRequest>,
) -> Result<(), ApiError> {
    unimplemented!()
    // let handle = {
    //     match app_state
    //         .services_runtime_stats
    //         .write()
    //         .get(&new_filter.service_id)
    //     {
    //         None => {
    //             return Err(ApiError {
    //                 code: StatusCode::NOT_FOUND,
    //                 message: format!("Service doesn't exist: {:#?}", new_filter.service_id),
    //             });
    //         }
    //         Some(handle) => match handle.instances.get(&new_filter.instance_id) {
    //             None => {
    //                 return Err(ApiError {
    //                     code: StatusCode::NOT_FOUND,
    //                     message: format!(
    //                         "Service exists, but no instance given id {} running",
    //                         new_filter.instance_id
    //                     ),
    //                 });
    //             }
    //             Some(instance_state) => instance_state.see_handle.clone(),
    //         },
    //     }
    // };
    // return match handle
    //     .send(ChangeFilterInternalRequest {
    //         filters: new_filter.filters,
    //     })
    //     .await
    // {
    //     Ok(_sent) => Ok(()),
    //     Err(_e) => Err(ApiError {
    //         code: StatusCode::BAD_REQUEST,
    //         message: format!(
    //             "Instance with id {} is no longer connected",
    //             new_filter.instance_id
    //         ),
    //     }),
    // };
}

/// Used to get information of one of the current services we data for, current or past
#[instrument(skip_all)]
pub async fn get(
    State(app_state): State<AppState>,
) -> Result<Json<<api_structs::ui::service::GetService as Endpoint>::ResponseBody>, ApiError> {
    let con = app_state.con;
    let service_data: Vec<serde_json::value::Value> = sqlx::query_scalar!(
        "select json_object(
               'id' : service.id,
               'name' : service.name,
               'env' : service.env,
               'log_filter' : service_rust_log_setting.rust_log,
               'instances' :
               json_agg(json_object(
                       'id'::text : instance.id,
                       'log_filter' : instance_latest_log_filter_and_cpu_profile.log_filter,
                       'registered_at' : instance.registered_at,
                       'last_updated_at' : last_instance_update.created_at,
                       'export_buffer_size_bytes' : last_instance_update.export_buffer_size_bytes,
                       'has_profile_data' :
                       instance_latest_log_filter_and_cpu_profile.cpu_profile is not null
                        ))
       ) as \"service_data!\"
from service
         left join service_rust_log_setting on service_rust_log_setting.service_id = service.id
         left join instance on instance.service_id = service.id
         left join lateral (select *
                            from instance_update
                            where instance_update.instance_id = instance.id
                            order by instance_update.created_at desc
                            limit 1) as last_instance_update on true
         left join instance_latest_log_filter_and_cpu_profile
                   on instance_latest_log_filter_and_cpu_profile.instance_id = instance.id
group by service.id, service_rust_log_setting.service_id;"
    )
    .fetch_all(&con)
    .await
    .map_err(SqlxError::from)?;

    let res: Vec<api_structs::ui::service::Service> = service_data
        .into_iter()
        .map(|val| {
            let val_str = val.to_string();
            serde_json::from_value(val)
                .map_err(|err| SerdeJsonError::from_serde_json_error(err, val_str))
        })
        .collect::<Result<Vec<api_structs::ui::service::Service>, SerdeJsonError>>()?;
    Ok(Json(res))
}
