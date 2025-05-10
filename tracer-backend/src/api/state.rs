#[derive(Clone)]
pub struct AppState {
    pub gel_client: gel_tokio::Client,
    pub execution_io_provider: crate::io_provider::ExecutionIoProvider,
}
