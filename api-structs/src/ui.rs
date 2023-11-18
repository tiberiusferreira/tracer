pub mod live_services;
pub mod orphan_events;
pub mod search_grid;
pub mod trace_view;
pub type ServiceName = String;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewFiltersRequest {
    pub instance_id: i64,
    pub filters: String,
}
