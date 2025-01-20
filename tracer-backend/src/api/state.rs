#[derive(Clone)]
pub struct AppState {
    pub edgedb_client: edgedb_tokio::Client,
}
