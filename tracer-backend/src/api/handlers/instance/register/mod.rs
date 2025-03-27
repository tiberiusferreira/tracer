use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::ServiceId;
use api_structs::instance::registration::RegistrationResponse;
use axum::Json;
use axum::extract::State;
use gel_tokio::RetryingTransaction;
use tracing::{info, instrument};

// edgedb_query!(
//     insert_service,
//     "
// with
//   env := <str>$env,
//   name := <str>$name,
//   service := (
//       insert Service{
//               env := env,
//               name := name,
//               log_filter := (
//                 insert LogFilter {
//                   _value := 'info'
//                 }
//               )
//             }
//       unless conflict on (.env, .name)
//       else
//         (select Service)
//   ),
//   service_instance := (
//     insert ServiceInstance{
//       service := service,
//       latest_log_filter := (
//         insert LogFilter {
//                   _value := 'info'
//             }
//       )
//     }
//   )
// select {
//   service := service {
//     log_filter_value:= service.log_filter._value
//   },
//   service_existed := (service in Service),
//   service_instance := service_instance
// };
// "
// );

use gel_tokio::Queryable;

#[derive(Queryable)]
struct InstanceInsertionData {
    service_instance_id: uuid::Uuid,
    log_filter: String,
}
async fn register_instance(
    mut tx: RetryingTransaction,
    env: &str,
    service: &str,
) -> Result<InstanceInsertionData, gel_tokio::Error> {
    let args = gel_protocol::named_args! {
        "env" => env,
        "name" => service,
    };
    let instance_insertion_data: InstanceInsertionData = tx
        .query_required_single(
            r#"
            with
   env := <str>$env,
   name := <str>$name,
   log_filter := (
                  insert LogFilter {
                   _value := 'info'
                  } unless conflict on (._value)
                  else
                   (select LogFilter)
                ),
   service := (
       insert Service{
               env := env,
               name := name,
               log_filter := log_filter
             }
       unless conflict on (.env, .name)
       else
         (select Service)
   ),
   service_instance := (
     insert ServiceInstance{
       service := service,
       latest_log_filter := log_filter
     }
   )
 select {
   log_filter := service.log_filter._value,
   service_instance_id := service_instance.id
 };"#,
            &args,
        )
        .await?;
    Ok(instance_insertion_data)
}
#[instrument(skip_all)]
pub async fn handler(
    app_state: State<AppState>,
    service_id: Json<ServiceId>,
) -> Result<Json<RegistrationResponse>, ApiError> {
    let service_id = service_id.0;
    info!(service.name=service_id.name, service.env=?service_id.env,  "registration request for service");

    let mut instance_insertion_data = app_state
        .gel_client
        .transaction(|tx| register_instance(tx, &service_id.env, &service_id.name))
        .await?;
    info!(
        service_instance_id = %instance_insertion_data.service_instance_id,
        "registered"
    );
    Ok(Json(RegistrationResponse {
        instance_id: instance_insertion_data.service_instance_id,
        log_filter: instance_insertion_data.log_filter,
    }))
}
