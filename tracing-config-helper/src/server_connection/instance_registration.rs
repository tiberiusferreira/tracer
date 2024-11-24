use crate::print_if_dbg;
use crate::server_connection::Error;
use api_structs::instance::connect::RegistrationResponse;
use tracked_error::{ReqwestError, SerdeJsonError};

const REGISTRATION_ENDPOINT: &str = "/api/instance/register";

pub async fn register_instance(
    client: &reqwest::Client,
    collector_url: &str,
    timeout: std::time::Duration,
) -> Result<RegistrationResponse, Error> {
    let context = "register_instance";
    let registration_endpoint = format!("{}{}", collector_url, REGISTRATION_ENDPOINT);
    print_if_dbg(
        context,
        format!("sending request to {registration_endpoint} with timeout: {timeout:?}"),
    );
    let request = client.get(registration_endpoint);
    let response = request
        .timeout(timeout)
        .send()
        .await
        .map_err(ReqwestError::from)?;
    let status = response.status();
    print_if_dbg(context, format!("got status: {status}"));
    let body = response.text().await.map_err(ReqwestError::from)?;
    let response: RegistrationResponse =
        serde_json::from_str(&body).map_err(|e| Error::UnexpectedResponseBody {
            error: SerdeJsonError::from_serde_json_error(e, body.chars().take(200).collect()),
            status,
        })?;
    Ok(response)
}
