use chrono::{DateTime, NaiveDateTime};

pub const NANOS_IN_1_MS: u64 = 1_000_000;
pub const NANOS_IN_1_SEC: u64 = 1_000_000_000;

// doesnt panic
pub fn time_from_nanos(nanos: u64) -> NaiveDateTime {
    let timestamp = DateTime::from_timestamp(
        i64::try_from(nanos / NANOS_IN_1_SEC)
            .expect("u64 should always fit i64 after division by nanos_in_1_sec"),
        u32::try_from(nanos % NANOS_IN_1_SEC).unwrap(),
    )
    .unwrap()
    .naive_utc();
    timestamp
}

#[test]
fn time_from_nanos_doesnt_panic() {
    println!("{}", time_from_nanos(u64::MAX));
    println!("{}", time_from_nanos(0));
}

// doesn't panic before the year 2200
pub fn now_nanos_u64() -> u64 {
    time_to_nanos_u64(chrono::Utc::now().naive_utc())
}

fn time_to_nanos_u64(time: NaiveDateTime) -> u64 {
    u64::try_from(
        time.and_utc()
            .timestamp_nanos_opt()
            .expect("current time in nanos to fit i64 until 2262 or so"),
    )
    .expect("current time in nanos to be positive")
}

pub fn nanos_to_millis(nanos: u64) -> u64 {
    nanos / NANOS_IN_1_MS
}
pub fn nanos_to_secs(nanos: u64) -> u64 {
    nanos / NANOS_IN_1_SEC
}
