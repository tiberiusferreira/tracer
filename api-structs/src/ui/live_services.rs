use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveInstances {
    pub instances: HashMap<crate::ui::ServiceName, Vec<LiveServiceInstance>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveServiceInstance {
    pub last_seen_timestamp: u64,
    pub service_id: i64,
    pub service_name: String,
    pub filters: String,
    pub tracer_stats: crate::TracerStats,
}
