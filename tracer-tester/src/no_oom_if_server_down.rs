use std::time::Duration;

use tracing::{info, instrument};

use tracing_config_helper::{Env, ServiceId, TracerConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    std::env::set_var("RUST_LOG", "trace");
    let tracer_config = TracerConfig::new(
        ServiceId {
            name: env!("CARGO_BIN_NAME").to_string(),
            env: Env::Local,
        },
        "http://127.0.0.1:4123".to_string(),
    );
    let _flush_requester = tracing_config_helper::setup_tracer_client_or_panic(tracer_config).await;
    loop {
        info!("sample info log");
        sample_function();
        tokio::time::sleep(Duration::from_secs_f32(0.1)).await;
    }
}

#[instrument]
fn sample_function() {
    info!("sample event inside span");
}
