#[derive(Clone)]
pub struct AppState {
    pub execution_io_provider: tracing_config_helper::io_provider::ExecutionIoProvider,
}
