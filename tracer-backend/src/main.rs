use clap::Parser;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::fmt::{Debug, Formatter};
use std::str::FromStr;
use std::thread;
use std::time::Duration;
use tracing::{info, info_span, instrument, Instrument};
use tracing_config_helper::TracerConfig;
mod api;
mod notification_worthy_events;

pub const BYTES_IN_1MB: usize = 1_000_000;
pub const TIME_WAIT_BETWEEN_DELETE_TRACES_RUN_SECONDS: u64 = 60;
pub const SINGLE_EVENT_CHARS_LIMIT: usize = 1_500_000;
pub const SINGLE_KEY_VALUE_VALUE_CHARS_LIMIT: usize = 1_500_000;
pub const SINGLE_KEY_VALUE_KEY_CHARS_LIMIT: usize = 256;

// ~10 span+logs per trace, 2 traces per second = 20 span+logs per second
pub const SPAN_PLUS_EVENTS_PER_SERVICE_PER_SECOND_NOTIFICATION_THRESHOLD: usize = 20;

pub const MAX_STATS_HISTORY_DATA_COUNT: usize = 500;

#[derive(Debug, clap::Parser)]
pub struct Config {
    #[clap(flatten)]
    pub db: DbConfig,
    #[clap(long, env, default_value_t = 4317)]
    pub collector_listen_port: u16,
    #[clap(long, env, default_value_t = 4200)]
    pub api_listen_port: u16,
    #[clap(long, env)]
    pub environment: String,
    #[clap(long, env)]
    pub slack_notification_url: Option<String>,
    #[clap(long, env, default_value_t = 3600)]
    pub slack_notification_interval_seconds: u32,
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
            .finish()
    }
}

async fn connect_to_db(config: &Config) -> Result<PgPool, Box<dyn std::error::Error>> {
    let con = PgPoolOptions::new()
        .max_connections(u32::from(config.db.max_db_connections))
        .connect_with(PgConnectOptions::from_str(&config.db.url).expect("to have a valid DB url"))
        .instrument(info_span!("Connecting to the DB"))
        .await?;
    Ok(con)
}

// This should not run forever, otherwise we lose the trace of starting up
#[instrument(level = "error", skip_all)]
async fn start_tasks(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    info!("Using config: {:#?}", config);
    let con = connect_to_db(&config).await?;
    let _api_handle = api::start(con.clone(), config.api_listen_port);
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let current_thread_runner = tokio::task::LocalSet::new();
    current_thread_runner
        .run_until(async {
            // load env vars so clap can use it when parsing a config
            println!("Loading env vars");
            dotenv::dotenv().ok();
            let config = Config::parse();
            let tracer_config = TracerConfig::new(
                api_structs::Env::Local,
                env!("CARGO_BIN_NAME").to_string(),
                "http://127.0.0.1:4200".to_string(),
            );
            tracing_config_helper::setup_tracer_client_or_panic(tracer_config).await;
            start_tasks(config)
                .await
                .expect("failed to start server and tasks");
            tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
        })
        .await
}
