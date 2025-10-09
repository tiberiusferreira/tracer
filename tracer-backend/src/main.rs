use std::fmt::Debug;
use std::time::Duration;

use crate::api::state::AppState;
use api_structs::ServiceId;
use clap::Parser;
use gel_io_recorder::DatabaseIoRecorder;
use tokio::task::spawn_local;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use tracer::io_provider::execution_recorder::record_single_attribute;
use tracer::TracerConfig;
use tracked_error::error_chain_to_pretty_formatted;
use crate::api::handlers::instance::update::GelError;

mod api;
mod background_tasks;
mod series;
mod inbound;


#[derive(Debug, Clone, clap::Parser)]
pub struct LaunchConfig {
    #[clap(long, env, default_value_t = 4317)]
    pub collector_listen_port: u16,
    #[clap(long, env, default_value_t = 4200)]
    pub api_listen_port: u16,
    #[clap(long, env, default_value_t = {"local".to_string()})]
    pub environment: String,
}


#[tokio::main(flavor = "current_thread")]
async fn main() {
    // load env vars so clap can use it when parsing a config
    eprintln!("Loading env vars");
    dotenvy::dotenv().ok();
    eprintln!("Loaded");
    tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();
    let launch_config = LaunchConfig::parse();
    let tracer_config = TracerConfig::new(
        ServiceId {
            name: env!("CARGO_BIN_NAME").to_string(),
            env: launch_config.environment.clone(),
        },
        format!("http://127.0.0.1:{}", launch_config.api_listen_port),
    );
    let current_thread_runner = tokio::task::LocalSet::new();
    current_thread_runner
        .run_until(async {
            let join_handle = start_api_and_background_tasks(launch_config.clone())
                .await
                .expect("failed to start server and tasks");
            let tracer_flush_request =
                tracer::setup_server_exporter_task_or_panic(tracer_config).await;
            std::mem::forget(tracer_flush_request);
            join_handle
                .await
                .expect("api and background tasks shouldn't ever return");
        })
        .await;
}

async fn start_api_and_background_tasks(
    config: LaunchConfig,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    let edgedb_client = gel_tokio::create_client().await.unwrap();
    let execution_io_provider = DatabaseIoRecorder::Live(edgedb_client);
    let app_state = AppState {
        execution_io_provider: execution_io_provider.clone(),
    };
    let api_handle = api::start(app_state.clone(), config.api_listen_port);
    spawn_local(async move {
        // Sleep before tasks so they start after tracer is setup and we dont lose any traces
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            let task = async {
                info!("Cleaning up old traces");
                let outcome: Result<(), GelError> = async {
                    let mut tx = execution_io_provider.transaction_start().await?;
                    background_tasks::clean_up::delete_old(&mut tx).await?;
                    tx.commit().await?;
                    Ok(())
                }
                    .await;
                if let Err(e) = outcome {
                    let e = error_chain_to_pretty_formatted(&e);
                    error!("Error cleaning up old traces: {e}");
                    record_single_attribute("error".to_string(), e.to_string());
                }
            };
            tracer::record_execution_simple(task, true).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });

    Ok(api_handle)
}
