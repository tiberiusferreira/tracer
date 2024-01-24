#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SseRequest {
    NewFilter { filter: String },
}
