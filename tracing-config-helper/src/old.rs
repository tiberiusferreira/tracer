use api_structs::exporter::SamplerLimits;
use opentelemetry::sdk::trace::Tracer;
use std::fmt::Debug;
use std::time::Duration;
use tracing::subscriber::{self};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer};

/// This is a guard that will shutdown the OpenTelemetry exporter on drop.
/// TLDR; Keep this around in main to make sure it is dropped after the
/// program exits, due to regular causes our panic.
///
/// This is intended to be dropped after all the rest of the program
/// has finished running. This is also useful to be kept around
/// in programs intended to never exit (webservers for example)
/// because in case of a panic this gets dropped and the panic trace
/// is exported after the stack unwinding gets to main
#[derive(Debug)]
pub struct TraceShutdownGuard {
    tracer: Tracer,
}

impl Drop for TraceShutdownGuard {
    fn drop(&mut self) {
        match self.tracer.provider() {
            None => {
                panic!(
                    "TraceShutdownGuard dropped, but no tracer registered, this is likely a bug!"
                );
            }
            Some(provider) => {
                for export_res in provider.force_flush() {
                    if let Err(err) = export_res {
                        println!(
                            "Failed to export traces during TraceShutdownGuard drop, please, look into it: {:?}",
                            err
                        );
                    }
                }
            }
        }
        println!("Tracer is shutting down because the handle was dropped, traces will no longer be exported!");
        opentelemetry::global::shutdown_tracer_provider();
    }
}

/// Uses RUST_LOG, see https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html
/// on how to configure that. If not present, defaults to plain "info".
/// Registers two "tracer processor", one for logging to stdout and one that exports to a collector
/// Also registers a panic hook to try to export traces on panics
/// Pay attention to the limits, specially max 1024 events (logs) per span and try to only create spans
/// useful to debugging
/// Careful with span creations in loops as they can make the output hard to read or too big to visualize
pub async fn setup_or_panic(service_name: String, environment: String, collector_endpoint: String) {
    if service_name.trim().is_empty() {
        panic!("Service name can't be empty.");
    }
    if environment.trim().is_empty() {
        panic!("Environment can't be empty. Example: local, dev, stage, prod");
    }
    let service_name_with_env = format!("{service_name}-{environment}");
    println!(
        "Initializing tracing for service: {service_name_with_env}, \
    sending to collector at: {}",
        collector_endpoint
    );
    static ONCE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    match ONCE.set(true) {
        Ok(_) => {
            println!("Initializing tracing for {}", &service_name_with_env);
            crate::setup_tracer_client_or_panic(crate::TracerConfig {
                collector_url: "ws://127.0.0.1:4200/websocket/collector".to_string(),
                env: environment,
                service_name,
                filters: std::env::var("RUST_LOG").unwrap_or_default(),
                export_timeout: Duration::from_secs(1),
                status_send_period: Duration::from_secs(1),
                maximum_spe_buffer: 10_000,
                sampler_limits: SamplerLimits {
                    span_plus_event_per_minute_per_trace_limit: 1000,
                    logs_per_minute_limit: 1000,
                },
            })
            .await;
            println!("Tracing initialized");
        }
        Err(_) => {
            panic!("Tried to initialize tracing again, please, don't do this");
        }
    }
}

pub fn setup_tracing_console_logging_for_test() {
    let filter = {
        let env_filter = EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|e| {
            let default_filter = "info";
            println!(
                "Missing or invalid RUST_LOG, defaulting to {default_filter}. {:#?}",
                e
            );
            EnvFilter::builder()
                .parse(default_filter)
                .unwrap_or_else(|_| panic!("{default_filter} should work as filter"))
        });
        println!("Using env filter: {}", env_filter);
        env_filter
    };
    let fmt = tracing_subscriber::fmt::layer()
        // for tests ansi if nice
        .with_ansi(true)
        .compact()
        .with_filter(filter);
    let subscriber = tracing_subscriber::Registry::default().with(fmt);
    subscriber::set_global_default(subscriber).unwrap();
}

#[allow(unused)]
fn install_global_export_traces_on_panic_hook() {
    let current = std::panic::take_hook();
    println!("Installing panic hook");
    std::panic::set_hook(Box::new(move |panic_info| {
        println!("Running panic hook, trying to export creating and exporting panic span.");
        // Make sure we signal that we panic
        let panic_span = tracing::info_span!("program panicked", is_panic = true);
        panic_span.in_scope(|| {
            let bt = std::backtrace::Backtrace::force_capture();
            let panic_info: String = panic_info.to_string().chars().take(28_000).collect();
            let bt: String = bt.to_string().chars().take(28_000).collect();
            tracing::error!("Code panicked: Panic info: {}.", panic_info);
            tracing::error!("Backtrace:\n{bt}.");
        });
        current(panic_info)
    }));
}
