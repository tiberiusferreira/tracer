use std::fmt::Debug;
use std::time::Duration;

use crate::api::state::AppState;
use api_structs::ServiceId;
use clap::Parser;
use gel_io_recorder::{DatabaseIoRecorder, ExecutionIoProvider};
use tokio::task::spawn_local;
use tracing_config_helper::TracerConfig;

mod api;
mod background_tasks;
mod series;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let current_thread_runner = tokio::task::LocalSet::new();
    current_thread_runner
        .run_until(async {
            // load env vars so clap can use it when parsing a config
            println!("Loading env vars");
            dotenvy::dotenv().ok();
            let launch_config = LaunchConfig::parse();
            let join_handle = start_api_and_background_tasks(launch_config.clone())
                .await
                .expect("failed to start server and tasks");
            let tracer_config = TracerConfig::new(
                ServiceId {
                    name: env!("CARGO_BIN_NAME").to_string(),
                    env: launch_config.environment.clone(),
                },
                format!("http://127.0.0.1:{}", launch_config.api_listen_port),
            );

            let tracer_flush_request =
                tracing_config_helper::setup_server_exporter_task_or_panic(tracer_config).await;
            std::mem::forget(tracer_flush_request);
            join_handle
                .await
                .expect("api and background tasks shouldn't ever return");
        })
        .await
}

async fn start_api_and_background_tasks(
    config: LaunchConfig,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    let edgedb_client = gel_tokio::create_client().await.unwrap();
    let app_state = AppState {
        execution_io_provider: ExecutionIoProvider {
            database: DatabaseIoRecorder::Live(edgedb_client),
        },
    };
    let api_handle = api::start(app_state.clone(), config.api_listen_port);
    spawn_local(async move {
        // Sleep before tasks so they start after tracer is setup and we dont lose any traces
        tokio::time::sleep(Duration::from_secs(3)).await;

        loop {
            async {
                // TODO remove old traces
            }
            .await;
            tokio::time::sleep(Duration::from_secs(60 * 60 * 60)).await;
        }
    });

    Ok(api_handle)
}

#[derive(Debug, Clone, clap::Parser)]
pub struct LaunchConfig {
    #[clap(long, env, default_value_t = 4317)]
    pub collector_listen_port: u16,
    #[clap(long, env, default_value_t = 4200)]
    pub api_listen_port: u16,
    #[clap(long, env, default_value_t = {"local".to_string()})]
    pub environment: String,
}
