#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SseRequest {
    NewFilter { filter: String },
}

// #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
// pub struct NewFilter {
//     pub filter: String,
// }
