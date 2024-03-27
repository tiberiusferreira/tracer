use reqwest::StatusCode;
use thiserror::Error;

use api_structs::instance::update::Sampling;

use crate::server_connection::request_compression::compress_and_set_body_and_with_encoding_headers;
use crate::UPDATE_ENDPOINT;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Got unexpected response: {response}")]
    UnexpectedResponse {
        response: String,
        #[source]
        error: serde_json::Error,
    },
    #[error("Got non 200 status code {status} and body: {body}")]
    Non200Status { status: StatusCode, body: String },
    #[error("http error")]
    Http(#[from] reqwest::Error),
}

pub async fn export_instance_update(
    client: &reqwest::Client,
    collector_url: &str,
    export_data: &str,
    export_timeout: core::time::Duration,
) -> Result<Sampling, Error> {
    let request = client.post(format!("{}{}", collector_url, UPDATE_ENDPOINT));
    let request = compress_and_set_body_and_with_encoding_headers(request, &export_data);
    return match request
        .header("Content-Type", "application/json")
        .timeout(export_timeout)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let body = response.text().await.map_err(|e| Error::Http(e))?;
            let response: Sampling =
                serde_json::from_str(&body).map_err(|e| Error::UnexpectedResponse {
                    response: body.chars().take(200).collect(),
                    error: e,
                })?;
            Ok(response)
        }
        Ok(response) => {
            let status = response.status();
            Err(Error::Non200Status {
                status,
                body: response
                    .text()
                    .await
                    .unwrap_or_else(|e| format!("error decoding body: {:?}", e)),
            })
        }
        Err(e) => Err(Error::Http(e)),
    };
}
