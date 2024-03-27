use sqlx::types::JsonValue;

pub mod chunk;
pub mod grid;

struct RawDbSpan {
    id: i64,
    timestamp: i64,
    parent_id: Option<i64>,
    duration: Option<i64>,
    name: String,
    key_values: JsonValue,
    module: Option<String>,
    filename: Option<String>,
    line: Option<i64>,
}

struct RawDbEvent {
    span_id: i64,
    message: Option<String>,
    severity: String,
    timestamp: i64,
    key_values: JsonValue,
    module: Option<String>,
    filename: Option<String>,
    line: Option<i64>,
}
