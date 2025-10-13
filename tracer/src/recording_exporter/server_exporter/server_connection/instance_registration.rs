use reqwest::StatusCode;
use thiserror::Error;
use tracing::info;
use api_structs::instance::registration::RegistrationResponse;
use api_structs::{Endpoint, ServiceId};
use tracked_error::{ReqwestError, SerdeJsonError};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Unexpected status: {status} Body {body}")]
    NonOkResponse { status: StatusCode, body: String },
    #[error("Unexpected response body. Status: {status}")]
    UnexpectedResponseBody {
        #[source]
        error: SerdeJsonError,
        status: StatusCode,
    },
    #[error("Http error")]
    Http(#[from] ReqwestError),
}

pub async fn register_instance(
    client: &reqwest::Client,
    collector_url: &str,
    service_id: &ServiceId,
    timeout: std::time::Duration,
) -> Result<RegistrationResponse, Error> {
    let context = "register_instance";
    let path = api_structs::instance::registration::RegistrationEndpoint::PATH;
    let registration_endpoint = format!("{collector_url}{path}");
    info!(
        context,
        "sending request to {registration_endpoint} with timeout: {timeout:?}"
    );
    let request = client.post(registration_endpoint).json(service_id);
    let response = request
        .timeout(timeout)
        .send()
        .await
        .map_err(ReqwestError::from)?;
    let status = response.status();
    info!(context, "got status: {status}");
    let body = response.text().await.map_err(ReqwestError::from)?;
    let response: RegistrationResponse =
        serde_json::from_str(&body).map_err(|e| Error::UnexpectedResponseBody {
            error: SerdeJsonError::from_serde_json_error(e, body.chars().take(200).collect()),
            status,
        })?;
    Ok(response)
}
