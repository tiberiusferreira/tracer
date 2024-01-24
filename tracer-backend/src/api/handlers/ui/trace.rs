use sqlx::types::JsonValue;
pub mod chunk;
pub mod grid;

struct RawDbSpan {
    id: i64,
    timestamp: i64,
    parent_id: Option<i64>,
    duration: Option<i64>,
    name: String,
    relocated: bool,
    key_values: JsonValue,
}

struct RawDbEvent {
    span_id: i64,
    message: Option<String>,
    severity: String,
    relocated: bool,
    timestamp: i64,
    key_values: JsonValue,
}
