#[derive(Clone)]
pub struct AppState {
    pub edgedb_client: gel_tokio::Client,
}
