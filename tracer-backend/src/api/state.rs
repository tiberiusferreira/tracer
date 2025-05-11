#[derive(Clone)]
pub struct AppState {
    pub gel_client: gel_tokio::Client,
    pub execution_io_provider: tracing_config_helper::io_provider::ExecutionIoProvider,
}
