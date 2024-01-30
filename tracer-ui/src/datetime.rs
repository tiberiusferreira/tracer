use api_structs::time_conversion::{time_from_nanos, NANOS_IN_1_MS, NANOS_IN_1_SEC};
use chrono::{Duration, NaiveDateTime};

pub fn printable_local_date(timestamp: u64) -> String {
    let timestamp = time_from_nanos(timestamp);
    let offset_minutes = js_sys::Date::new_0().get_timezone_offset() as i64;
    utc_to_local_date(timestamp, offset_minutes)
        .format("%m-%d %H:%M:%S")
        .to_string()
}

pub fn utc_to_local_date(utc: NaiveDateTime, offset_minutes: i64) -> NaiveDateTime {
    utc - Duration::minutes(offset_minutes)
}
pub fn local_date_to_utc(local: NaiveDateTime, offset_minutes: i64) -> NaiveDateTime {
    local + Duration::minutes(offset_minutes)
}

pub fn printable_local_date_ms(timestamp: u64) -> String {
    let timestamp = time_from_nanos(timestamp);
    let offset_minutes = js_sys::Date::new_0().get_timezone_offset() as i64;
    utc_to_local_date(timestamp, offset_minutes)
        .format("%m-%d %H:%M:%S%.6f")
        .to_string()
}

pub fn secs_since(timestamp: u64) -> u64 {
    let timestamp_ms = js_sys::Date::now() as u64;
    let nanos = (timestamp_ms * NANOS_IN_1_MS)
        .checked_sub(timestamp)
        .unwrap();
    let secs = nanos / NANOS_IN_1_SEC;
    secs
}
