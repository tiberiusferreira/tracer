use chrono::NaiveDateTime;
use sqlx::types::JsonValue;

pub mod chunk;
pub mod event_search;
pub mod grid;

struct RawDbSpan {
    id: i32,
    timestamp: NaiveDateTime,
    parent_id: Option<i32>,
    duration_nanos: i64,
    name: String,
    key_values: JsonValue,
    module: Option<String>,
    filename: Option<String>,
    line: Option<i32>,
}

struct RawDbEvent {
    span_id: i32,
    message: Option<String>,
    severity: String,
    timestamp: NaiveDateTime,
    key_values: JsonValue,
    module: Option<String>,
    filename: Option<String>,
    line: Option<i32>,
}
