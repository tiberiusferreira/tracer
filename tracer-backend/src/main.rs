use std::fmt::{Debug, Formatter};
use std::time::Duration;

use crate::api::state::AppState;
use api_structs::ServiceId;
use clap::Parser;
use gel_io_recorder::{DatabaseIoRecorder, ExecutionIoProvider};
use tokio::task::spawn_local;
use tracing_config_helper::TracerConfig;

mod api;
mod background_tasks;
mod notification_worthy_events;
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

// This should not run forever, otherwise we lose the trace of starting up
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
                // let state = app_state.clone();
                println!("Checking for check_for_alerts_and_send");

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
            .await;
            tokio::time::sleep(Duration::from_secs(60 * 60 * 60)).await;
        }
    });

    Ok(api_handle)
}

#[derive(Debug, Clone, clap::Parser)]
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

#[derive(Clone, clap::Parser)]
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

// #[test]
// fn a() {
//     let bytes = std::fs::read(
//         // "/Users/tiberiodarferreira/Documents/github/tracer/data_being_exported_3.json",
//         "/Users/tiberiodarferreira/Documents/github/tracer/nest_1.json",
//     )
//     .unwrap();
//     let string = String::from_utf8(bytes).unwrap();
//     let bytes = serde_json::from_str::<Vec<u8>>(&string).unwrap();
//     let string = String::from_utf8(bytes).unwrap();
//     // println!("{:#?}", string);
//     fs::write("./out.json", string).unwrap();
// }
