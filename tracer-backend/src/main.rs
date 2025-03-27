use std::fmt::{Debug, Formatter};
use std::time::Duration;

use crate::api::state::AppState;
use api_structs::ServiceId;
use clap::Parser;
use tokio::task::spawn_local;
use tracing::{Instrument, info, info_span, instrument};
use tracing_config_helper::TracerConfig;
use valuable_derive::Valuable;
mod api;
mod background_tasks;
mod error;
mod notification_worthy_events;
mod series;

pub const BYTES_IN_1MB: usize = 1_000_000;
pub const SINGLE_EVENT_CHARS_LIMIT: usize = 1_500_000;
pub const DB_INTERNAL_ERROR_CHAR_LIMIT: usize = 4096;
pub const SINGLE_KEY_VALUE_VALUE_CHARS_LIMIT: usize = 1_500_000;
pub const SINGLE_KEY_VALUE_KEY_CHARS_LIMIT: usize = 256;
pub const DEAD_INSTANCE_RETENTION_TIME_SECONDS: usize = 12 * 60 * 60;
pub const DEAD_INSTANCE_MAX_STATS_HISTORY_DATA_COUNT: usize = 50;
pub const CONSIDER_DEAD_INSTANCE_AFTER_NO_DATA_FOR_SECONDS: usize = 12 * 60 * 60;

pub const MAX_STATS_HISTORY_DATA_COUNT: usize = 500;
pub const MAX_NOTIFICATION_SIZE_CHARS: usize = 2048;

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
            )
            .with_enable_log_exporting(true)
            .with_stdout_logging(true);

            let _tracer_flush_request =
                tracing_config_helper::setup_tracer_client_in_background_or_panic(tracer_config)
                    .await;
            join_handle
                .await
                .expect("api and background tasks shouldn't ever return");
        })
        .await
}

// This should not run forever, otherwise we lose the trace of starting up
#[instrument(level = "error", skip_all)]
async fn start_api_and_background_tasks(
    config: LaunchConfig,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    let edgedb_client = gel_tokio::create_client().await.unwrap();
    let app_state = AppState {
        gel_client: edgedb_client,
    };
    let api_handle = api::start(app_state.clone(), config.api_listen_port);
    spawn_local(async move {
        // Sleep before tasks so they start after tracer is setup and we dont lose any traces
        tokio::time::sleep(Duration::from_secs(3)).await;
        // info!(config = config.as_value(), "Using config");

        loop {
            async {
                // let state = app_state.clone();
                info!("Checking for check_for_alerts_and_send");

                // if let Err(e) =
                //     background_tasks::alerts::checker::execute_series_and_check_for_alerts(
                //         state.edgedb_client.clone(),
                //     )
                //     .await
                // {
                //     let error_chain_as_string = error_chain_to_pretty_formatted(&e);
                //     error!("{}", error_chain_as_string);
                // }
                // info!("Sending alerts");
                // if let Err(e) =
                //     background_tasks::alerts::senders::telegram::send_alerts(state.con.clone(), 3)
                //         .await
                // {
                //     let error_chain_as_string = error_chain_to_pretty_formatted(&e);
                //     error!("{}", error_chain_as_string);
                // }
                // background_tasks::clean_up::instance_runtime_data::clean_up_dead_instances_and_services(
                //     Arc::clone(&state.services_runtime_stats),
                // );
                // background_tasks::clean_up::database_old_traces_and_logs::delete_old_traces_logging_error(&state.con).await;
                // background_tasks::clean_up::database_old_traces_and_logs::delete_old_orphan_events_logging_error(&state.con).await;
                // background_tasks::clean_up::old_slack_notification::delete_old_slack_notifications_logging_error(&state.con).await;
            }
            .instrument(info_span!("background_task"))
            .await;
            tokio::time::sleep(Duration::from_secs(5 * 60)).await;
        }
    });

    Ok(api_handle)
}

#[derive(Debug, Clone, clap::Parser, Valuable)]
pub struct LaunchConfig {
    #[clap(flatten)]
    pub db: DbConfig,
    #[clap(long, env, default_value_t = 4317)]
    pub collector_listen_port: u16,
    #[clap(long, env, default_value_t = 4200)]
    pub api_listen_port: u16,
    #[clap(long, env)]
    pub environment: String,
}

#[derive(Clone, clap::Parser, Valuable)]
pub struct DbConfig {
    #[clap(long, env = "DATABASE_URL")]
    pub url: String,
    #[clap(long, env, default_value_t = 10)]
    pub max_db_connections: u16,
}

impl Debug for DbConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConfig")
            .field("max_db_connections", &self.max_db_connections)
            .field(
                "url",
                &self
                    .url
                    .chars()
                    .rev()
                    .take(15)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>(),
            )
            .finish()
    }
}
