use crate::api::ApiError;
use api_structs::instance::update::ConfigChange;
use api_structs::InstanceGlobalId;
use axum::http::StatusCode;
use axum::Json;
use sqlx::{Postgres, Transaction};
use tracing::{error, info};

pub mod database;

pub async fn get_instance_config_change(
    tx: &mut Transaction<'static, Postgres>,
    instance_id: InstanceGlobalId,
    current_instance_log_filter: &str,
) -> Result<Json<ConfigChange>, ApiError> {
    let service_log_filter: Option<String> =
        crate::api::handlers::instance::database::get_instance_service_log_filter(tx, instance_id)
            .await?;
    let service_log_filter = match service_log_filter {
        None => {
            error!(
                instance_id = instance_id.to_string(),
                "got instance update for instance without log filter"
            );
            return Err(ApiError {
                code: StatusCode::BAD_REQUEST,
                message: "instance not registered".to_string(),
            });
        }
        Some(log_filter) => log_filter,
    };
    let mut current: Vec<char> = current_instance_log_filter.chars().collect();
    let mut new: Vec<char> = service_log_filter.chars().collect();
    current.sort();
    new.sort();
    let log_filter_request = if current != new {
        info!(instance_id = instance_id.to_string(), "got log filter");
        Some(service_log_filter)
    } else {
        None
    };
    Ok(Json(ConfigChange {
        log_filter: log_filter_request,
    }))
}
