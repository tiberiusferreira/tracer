use std::time::Duration;
use thiserror::Error;
use tracing::{error, info, warn};
use tracing_config_helper::{Env, TracerConfig};

#[derive(Debug, Error)]
enum MyErr {}

#[derive(Debug)]
struct MyKey {
    my_key_val: i32,
}
fn simple_orphan_logs() {
    let key = MyKey { my_key_val: 1 };
    info!(?key, "info");
    let mut my_vec = vec![1];
    for i in 0..10_000 {
        my_vec.push(i);
    }
    let key = format!("{:?}", my_vec);
    // warn!(keyw = "somee");
    warn!(key, "somee");
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("Hello, world!");
    let tracer_config = TracerConfig::new(
        Env::Local,
        env!("CARGO_BIN_NAME").to_string(),
        "http://127.0.0.1:4200".to_string(),
    );
    let flush_requester = tracing_config_helper::setup_tracer_client_or_panic(tracer_config).await;
    simple_orphan_logs();
    // let mut my_vec = vec![1];
    // for i in 0..1_000_000 {
    //     my_vec.push(i);
    // }
    // info!(my_vec = ?my_vec);

    flush_requester
        .flush(Duration::from_secs(100))
        .await
        .unwrap();
}
