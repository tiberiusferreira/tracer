use api_structs::ServiceId;
use clap::Parser;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::fmt::{Debug, Formatter};
use std::str::FromStr;
use tracing::{info, info_span, instrument, Instrument};
use tracing_config_helper::TracerConfig;
mod api;
mod notification_worthy_events;

pub const BYTES_IN_1MB: usize = 1_000_000;
pub const SINGLE_EVENT_CHARS_LIMIT: usize = 1_500_000;
pub const SINGLE_KEY_VALUE_VALUE_CHARS_LIMIT: usize = 1_500_000;
pub const SINGLE_KEY_VALUE_KEY_CHARS_LIMIT: usize = 256;

pub const MAX_STATS_HISTORY_DATA_COUNT: usize = 500;

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
    let api_handle = api::start(con.clone(), config.api_listen_port);
    // TODO
    // let _clean_up_service_instances_task =
    //     clean_up_service_instances_task(app_state.live_instances.clone());
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
