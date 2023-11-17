use clap::Parser;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::fmt::{Debug, Formatter};
use std::str::FromStr;
use std::time::Duration;
use tokio::task::spawn_local;
use tracing::{info, info_span, instrument, trace, Instrument};
use tracing_config_helper::{Env, TracerConfig};

mod api;
mod notification_worthy_events;
// mod otel_trace_processing;
// mod proto_generated;

pub const BYTES_IN_1MB: usize = 1_000_000;
pub const MAX_BUFFERED_TRACES: u64 = 2000;
pub const MAX_SINGLE_TRACE_SIZE_BYTES: usize = 500 * BYTES_IN_1MB; // 500MB
pub const TIME_WAIT_BETWEEN_STORE_TRACES_RUN_SECONDS: u64 = 5;
pub const TIME_WAIT_BETWEEN_DELETE_TRACES_RUN_SECONDS: u64 = 60;
pub const TIME_WAIT_PANIC_TASKS_ON_STARTUP_SECONDS: u64 = 5;
pub const MAX_TIME_WAIT_NEW_TRACE_DATA_SECONDS: u64 = 5;
pub const MAX_COMBINED_SPAN_AND_EVENTS_PER_TRACE: usize = 2_000_000;
pub const EVENT_CHARS_LIMIT: usize = 32_000;

// ~10 span+logs per trace, 2 traces per second = 20 span+logs per second
pub const SPAN_PLUS_EVENTS_PER_SERVICE_PER_SECOND_NOTIFICATION_THRESHOLD: usize = 20;

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
#[instrument(skip_all)]
async fn start_tasks(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    info!("Using config: {:#?}", config);
    let con = connect_to_db(&config).await?;
    let _api_handle = api::start(con.clone(), config.api_listen_port);
    spawn_local(async {
        loop {
            trace!("background task iteration starting");
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current_thread_runner = tokio::task::LocalSet::new();
    current_thread_runner
        .run_until(async {
            // load env vars so clap can use it when parsing a config
            println!("Loading env vars");
            dotenv::dotenv().ok();
            if std::env::var("RUST_LOG").ok().is_none() {
                let new_env_var = "tracer_backend,tracing_config_helper=trace";
                std::env::set_var("RUST_LOG", new_env_var);
                println!(
                    "Overwrote RUST_LOG env var to {} because it was empty",
                    new_env_var
                )
            }
            let config = Config::parse();
            let tracer_config = TracerConfig::new(
                Env::Local,
                env!("CARGO_BIN_NAME").to_string(),
                "http://127.0.0.1:4200".to_string(),
            );
            tracing_config_helper::setup_tracer_client_or_panic(tracer_config).await;
            start_tasks(config).await?;
            tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
            Ok(())
        })
        .await
}

// pub struct TraceCollector {
//     /// We might get data for traces in "pieces" from multiple batches
//     /// this groups them and allows us to wait a little bit before processing them
//     /// to give time for other parts to arrive
//     trace_fragment_pusher: trace_fragment::Pusher,
// }

// #[tonic::async_trait]
// impl TraceService for TraceCollector {
//     #[instrument(skip_all)]
//     async fn new_otel_trace_fragment(
//         &self,
//         request: Request<ExportTraceServiceRequest>,
//     ) -> Result<Response<ExportTraceServiceResponse>, Status> {
//         let request = request.into_inner();
//         otel_trace_processing::stage_trace_fragment(request, &self.trace_fragment_pusher).await;
//         Ok(Response::new(ExportTraceServiceResponse {
//             partial_success: None,
//         }))
//     }
// }
