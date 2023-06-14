//! This serves as an unified config for projects  
//! It outputs pretty logs to the console stdout and stderr,
//! but also exports traces to a collector
//!

use opentelemetry::sdk::trace;
use opentelemetry::sdk::trace::{BatchConfig, Tracer};
use opentelemetry_otlp::WithExportConfig;
use std::str::FromStr;
use std::time::Duration;
use tracing::subscriber::{self};
use tracing_subscriber::filter::Directive;
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
pub fn setup_or_panic(
    service_name: String,
    environment: String,
    collector_endpoint: String,
    sample_rate_0_to_1: f64,
) -> TraceShutdownGuard {
    if service_name.trim().is_empty() {
        panic!("Service name can't be empty.");
    }
    if environment.trim().is_empty() {
        panic!("Environment can't be empty. Example: local, dev, stage, prod");
    }
    if sample_rate_0_to_1 > 1. || sample_rate_0_to_1 < 0. {
        panic!("Sample rate should be between 0 and 1");
    }
    let sample_rate_perc = sample_rate_0_to_1 * 100.;
    let service_name_with_env = format!("{service_name}-{environment}");
    println!(
        "Initializing tracing for service: {service_name_with_env}, \
    sampling at: {sample_rate_perc:.0}%, \
    sending to collector at: {}",
        collector_endpoint
    );
    static ONCE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    match ONCE.set(true) {
        Ok(_) => {
            println!("Initializing tracing for {}", &service_name_with_env);
            setup_or_panic_impl(
                service_name_with_env,
                collector_endpoint,
                sample_rate_0_to_1,
            )
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

fn setup_or_panic_impl(
    service_name_with_env: String,
    collector_endpoint: String,
    sample_rate_0_to_1: f64,
) -> TraceShutdownGuard {
    if service_name_with_env.trim().is_empty() {
        panic!("Service name shouldn't be empty!");
    }
    let open_tel_tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_batch_config(
            // we want to allow big traces for nightly background jobs for example
            // up to 500MB max for a single trace but also have a long queue
            // to not drop the smaller ones.
            // Most of these are the same as the default, but keeping it here
            // so it doesnt change on lib updates since this is important
            BatchConfig::default()
                .with_max_queue_size(2048)
                .with_scheduled_delay(Duration::from_secs(5))
                .with_max_export_batch_size(512)
                .with_max_export_timeout(Duration::from_secs(30))
                .with_max_concurrent_exports(4),
        )
        .with_trace_config(
            trace::Config::default()
                // we don't want to lose any event, if possible
                .with_max_events_per_span(500_000)
                .with_resource(opentelemetry::sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", service_name_with_env),
                ]))
                .with_sampler(trace::Sampler::TraceIdRatioBased(sample_rate_0_to_1)),
        )
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(collector_endpoint),
        )
        .install_batch(opentelemetry::runtime::Tokio)
        .unwrap();

    // closure because filter is not clone
    let get_filter = || {
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
        // make sure we can log panics
        let final_filter = env_filter.add_directive(
            Directive::from_str(&format!("{}=info", env!("CARGO_CRATE_NAME")))
                .expect("to be a valid filter"),
        );
        println!("Using env filter: {}", final_filter);
        final_filter
    };
    let fmt = tracing_subscriber::fmt::layer()
        // we normally look up logs in dash or kibana and it doesnt handle ansi, dash
        // throws it away, kibana shows weird characters
        // see: https://github.com/kubernetes/dashboard/issues/1035
        .with_ansi(false)
        .json()
        .with_filter(get_filter());
    let open_tel = tracing_opentelemetry::layer()
        // remove these extra attributes because they are generated
        // for _each_ span and event, generating _a lot_ of attributes
        // per event and span
        .with_threads(false)
        .with_tracked_inactivity(false)
        .with_location(false)
        .with_exception_fields(false)
        .with_exception_field_propagation(false)
        .with_tracer(open_tel_tracer.clone())
        .with_filter(get_filter());
    install_global_export_traces_on_panic_hook();
    let subscriber = tracing_subscriber::Registry::default()
        .with(open_tel)
        .with(fmt);
    subscriber::set_global_default(subscriber).unwrap();
    TraceShutdownGuard {
        tracer: open_tel_tracer,
    }
}

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
