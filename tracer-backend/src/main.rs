use crate::api::state::AppState;
use api_structs::ServiceId;
use backtraced_error::error_chain_to_pretty_formatted;
use clap::Parser;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::spawn_local;
use tracing::{error, info, info_span, instrument, Instrument};
use tracing_config_helper::TracerConfig;

mod api;
mod background_tasks;
mod database;
mod notification_worthy_events;

pub const BYTES_IN_1MB: usize = 1_000_000;
pub const SINGLE_EVENT_CHARS_LIMIT: usize = 1_500_000;
pub const DB_INTERNAL_ERROR_CHAR_LIMIT: usize = 4096;
pub const SINGLE_KEY_VALUE_VALUE_CHARS_LIMIT: usize = 1_500_000;
pub const SINGLE_KEY_VALUE_KEY_CHARS_LIMIT: usize = 256;
pub const DEAD_INSTANCE_RETENTION_TIME_SECONDS: usize = 12 * 60 * 60;
pub const DEAD_INSTANCE_MAX_STATS_HISTORY_DATA_COUNT: usize = 50;
pub const CONSIDER_DEAD_INSTANCE_AFTER_NO_DATA_FOR_SECONDS: usize = 60;

pub const MAX_STATS_HISTORY_DATA_COUNT: usize = 500;
pub const MAX_NOTIFICATION_SIZE_CHARS: usize = 2048;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let current_thread_runner = tokio::task::LocalSet::new();
    current_thread_runner
        .run_until(async {
            // load env vars so clap can use it when parsing a config
            println!("Loading env vars");
            dotenv::dotenv().ok();
            let config = LaunchConfig::parse();
            let env = tracing_config_helper::Env::from(config.environment.clone());
            let tracer_config = TracerConfig::new(
                ServiceId {
                    name: env!("CARGO_BIN_NAME").to_string(),
                    env,
                },
                format!("http://127.0.0.1:{}", config.api_listen_port),
            );
            let _tracer_flush_request =
                tracing_config_helper::setup_tracer_client_or_panic(tracer_config).await;
            let join_handle = start_api_and_background_tasks(config)
                .await
                .expect("failed to start server and tasks");
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
    info!("Using config: {:#?}", config);
    let con = connect_to_db(&config).await?;
    let app_state = AppState {
        con,
        services_runtime_stats: std::sync::Arc::new(parking_lot::RwLock::new(HashMap::new())),
    };
    let api_handle = api::start(app_state.clone(), config.api_listen_port);
    spawn_local(async move {
        loop {
            async {
                let state = app_state.clone();
                info!("Checking for check_for_alerts_and_send");
                if let Err(e) = background_tasks::alerts::check_for_alerts_and_send(&state).await {
                    let error_chain_as_string = error_chain_to_pretty_formatted(&e);
                    error!("{}", error_chain_as_string);
                }
                background_tasks::clean_up::instance_runtime_data::clean_up_dead_instances_and_services(
                    Arc::clone(&state.services_runtime_stats),
                );
                background_tasks::clean_up::database_old_traces_and_logs::delete_old_traces_logging_error(&state.con).await;
                background_tasks::clean_up::database_old_traces_and_logs::delete_old_orphan_events_logging_error(&state.con).await;
                background_tasks::clean_up::old_slack_notification::delete_old_slack_notifications_logging_error(&state.con).await;
            }
            .instrument(info_span!("background_task"))
            .await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });

    Ok(api_handle)
}

#[derive(Debug, clap::Parser)]
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

#[derive(clap::Parser)]
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

async fn connect_to_db(config: &LaunchConfig) -> Result<PgPool, Box<dyn std::error::Error>> {
    let con = PgPoolOptions::new()
        .max_connections(u32::from(config.db.max_db_connections))
        .connect_with(PgConnectOptions::from_str(&config.db.url).expect("to have a valid DB url"))
        .instrument(info_span!("Connecting to the DB"))
        .await?;
    Ok(con)
}
