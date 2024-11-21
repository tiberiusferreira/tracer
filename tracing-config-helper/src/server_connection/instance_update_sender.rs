use crate::print_if_dbg;
use crate::server_connection::request_compression::compress_and_set_body_and_with_encoding_headers;
use tracked_error::ReqwestError;

pub const UPDATE_ENDPOINT: &str = "/api/instance/update";
use crate::server_connection::Error;

pub async fn export_instance_update(
    client: &reqwest::Client,
    collector_url: &str,
    export_data: &str,
    export_timeout: core::time::Duration,
) -> Result<(), Error> {
    let context = "export_instance_update";
    let export_endpoint = format!("{}{}", collector_url, UPDATE_ENDPOINT);
    let request = client.post(&export_endpoint);
    print_if_dbg(
        context,
        format!("sending request to {export_endpoint} with timeout: {export_timeout:?}"),
    );
    let request = compress_and_set_body_and_with_encoding_headers(request, &export_data);
    let response = request
        .header("Content-Type", "application/json")
        .timeout(export_timeout)
        .send()
        .await
        .map_err(ReqwestError::from)?;
    let status = response.status();
    if status == reqwest::StatusCode::OK {
        Ok(())
    } else {
        Err(Error::UnexpectedStatus { status })
    }
}
