use tracing::info;
use super::request_compression::compress_and_set_body_and_with_encoding_headers;
use tracked_error::ReqwestError;

pub const UPDATE_ENDPOINT: &str = "/api/instance/update";
use super::instance_registration::Error;

pub async fn export_instance_update(
    client: &reqwest::Client,
    collector_url: &str,
    export_data_json: &str,
    export_timeout: core::time::Duration,
    service_name: String,
) -> Result<(), Error> {
    let context = "export_instance_update";
    let export_endpoint = format!("{}{}", collector_url, UPDATE_ENDPOINT);
    let request = client
        .post(&export_endpoint)
        .header("service-name", service_name);
    info!(
        context,
        "sending request to {export_endpoint} with timeout: {export_timeout:?}",
    );
    let request = compress_and_set_body_and_with_encoding_headers(request, &export_data_json);
    let response = request
        .header("Content-Type", "application/json")
        .timeout(export_timeout)
        .send()
        .await
        .map_err(ReqwestError::from)?;
    let status = response.status();
    info!(context, "got status: {status}");
    let body = response.text().await.map_err(ReqwestError::from)?;
    if status.as_u16() != 200 {
        Err(Error::NonOkResponse { status, body })
    } else {
        Ok(())
    }
}
