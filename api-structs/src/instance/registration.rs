use serde::{Deserialize, Serialize};

pub struct RegistrationEndpoint;

impl crate::Endpoint for RegistrationEndpoint {
    const PATH: &'static str = "/api/instance/register";
    const METHOD: &'static str = "POST";
    type RequestBody = crate::ServiceId;
    type QueryParameters = ();
    type ResponseBody = RegistrationResponse;
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RegistrationResponse {
    pub instance_id: uuid::Uuid,
}
