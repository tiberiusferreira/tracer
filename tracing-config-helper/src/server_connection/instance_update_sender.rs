use crate::print_if_dbg;
use crate::server_connection::request_compression::compress_and_set_body_and_with_encoding_headers;
use api_structs::instance::update::ConfigChange;
use tracked_error::{ReqwestError, SerdeJsonError};

pub const UPDATE_ENDPOINT: &str = "/api/instance/update";
use crate::server_connection::Error;

pub async fn export_instance_update(
    client: &reqwest::Client,
    collector_url: &str,
    export_data_json: &str,
    export_timeout: core::time::Duration,
) -> Result<ConfigChange, Error> {
    let context = "export_instance_update";
    let export_endpoint = format!("{}{}", collector_url, UPDATE_ENDPOINT);
    let request = client.post(&export_endpoint);
    print_if_dbg(
        context,
        format!("sending request to {export_endpoint} with timeout: {export_timeout:?}"),
    );
    let request = compress_and_set_body_and_with_encoding_headers(request, &export_data_json);
    let response = request
        .header("Content-Type", "application/json")
        .timeout(export_timeout)
        .send()
        .await
        .map_err(ReqwestError::from)?;
    let status = response.status();
    print_if_dbg(context, format!("got status: {status}"));
    let body = response.text().await.map_err(ReqwestError::from)?;
    let response: ConfigChange =
        serde_json::from_str(&body).map_err(|e| Error::UnexpectedResponseBody {
            error: SerdeJsonError::from_serde_json_error(e, body.chars().take(200).collect()),
            status,
        })?;
    Ok(response)
}
