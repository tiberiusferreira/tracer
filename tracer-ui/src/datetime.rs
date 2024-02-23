use api_structs::time_conversion::{time_from_nanos, NANOS_IN_1_MS, NANOS_IN_1_SEC};
use chrono::{Duration, NaiveDateTime};
use std::ops::Deref;
use std::sync::RwLock;
use tracing::info;

pub static PAGE_LOAD_TIMESTAMP: RwLock<Option<u64>> = RwLock::new(None);

pub fn printable_local_date(timestamp: u64) -> String {
    let timestamp = time_from_nanos(timestamp);
    let offset_minutes = js_sys::Date::new_0().get_timezone_offset() as i64;
    utc_to_local_date(timestamp, offset_minutes)
        .format("%m-%d %H:%M:%S")
        .to_string()
}

pub fn get_page_load_timestamp_nanos() -> u64 {
    let page_load_time = PAGE_LOAD_TIMESTAMP.read().unwrap().deref().clone();
    page_load_time.unwrap_or_else(|| {
        set_page_load_timestamp();
        let page_load_time = PAGE_LOAD_TIMESTAMP.read().unwrap().deref().clone().unwrap();
        page_load_time
    })
}

pub fn set_page_load_timestamp() {
    let timestamp_ms = js_sys::Date::now() as u64;
    let nanos = (timestamp_ms * NANOS_IN_1_MS);
    *PAGE_LOAD_TIMESTAMP.write().unwrap() = Some(nanos);
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
    // info!("input: {}", timestamp);
    let page_nanos = get_page_load_timestamp_nanos();
    // info!(" page: {}", get_page_load_timestamp_nanos());
    let nanos = page_nanos.checked_sub(timestamp).unwrap();
    let secs = nanos / NANOS_IN_1_SEC;
    secs
}
