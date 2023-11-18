use chrono::NaiveDateTime;

// doesnt panic
pub fn time_from_nanos(nanos: u64) -> NaiveDateTime {
    let nanos_in_1_sec = 1_000_000_000;
    let timestamp = NaiveDateTime::from_timestamp_opt(
        i64::try_from(nanos / nanos_in_1_sec)
            .expect("u64 should always fit i64 after division by nanos_in_1_sec"),
        u32::try_from(nanos % nanos_in_1_sec).unwrap(),
    )
    .unwrap();
    timestamp
}

#[test]
fn time_from_nanos_doesnt_panic() {
    println!("{}", time_from_nanos(u64::MAX));
    println!("{}", time_from_nanos(0));
}

// doesnt panic before the year 2200
pub fn now_nanos_u64() -> u64 {
    time_to_nanos_u64(chrono::Utc::now().naive_utc())
}

fn time_to_nanos_u64(time: NaiveDateTime) -> u64 {
    u64::try_from(
        time.timestamp_nanos_opt()
            .expect("current time in nanos to fit i64 until 2262 or so"),
    )
    .expect("current time in nanos to be positive")
}
