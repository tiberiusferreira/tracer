use serde::{Deserialize, Serialize};

pub mod grid;
pub mod search;
pub mod spans;
pub mod time_series;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceHeaderAndSpans {
    pub top_level_span_name: String,
    pub start: u64,
    pub duration: u64,
    pub spans: Vec<spans::Span>,
}
