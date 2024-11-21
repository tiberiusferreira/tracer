use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SseRequest {
    NewFilter { filter: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RegistrationResponse {
    pub instance_id: uuid::Uuid,
    pub log_filter: String,
}
