use chrono::NaiveDateTime;
use std::time::Instant;

pub fn time_from_nanos(nanos: u64) -> NaiveDateTime {
    let nanos_in_1_sec = 1_000_000_000;
    let timestamp = NaiveDateTime::from_timestamp_opt(
        i64::try_from(nanos / nanos_in_1_sec).expect("timestamp to fit i64"),
        u32::try_from(nanos % nanos_in_1_sec).unwrap(),
    )
    .unwrap();
    timestamp
}

pub fn time_to_nanos_i64(time: NaiveDateTime) -> i64 {
    time.timestamp_nanos_opt()
        .expect("time in nanos to fit i64")
}

pub fn time_to_nanos_u64(time: NaiveDateTime) -> u64 {
    u64::try_from(
        time.timestamp_nanos_opt()
            .expect("time in nanos to fit i64"),
    )
    .expect("time in nanos to fit u64")
}

pub fn nanos_to_db_i64(nanos: u64) -> i64 {
    i64::try_from(nanos).expect("nanos to fit i64")
}

pub fn db_i64_to_nanos(nanos: i64) -> u64 {
    u64::try_from(nanos).expect("nanos to fit u64")
}

pub fn now_nanos_u64() -> u64 {
    time_to_nanos_u64(chrono::Utc::now().naive_utc())
}

pub fn duration_u64_nanos_from_instant(instant: Instant) -> u64 {
    u64::try_from(instant.elapsed().as_nanos()).expect("duration in nanos to fit in u64")
}
