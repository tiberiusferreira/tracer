use crate::Severity;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServiceOrphanEventsRequest {
    pub service_name: String,
    pub from_date_unix: u64,
    pub to_date_unix: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrphanEvent {
    pub timestamp: u64,
    pub severity: Severity,
    pub message: Option<String>,
    pub key_vals: HashMap<String, String>,
}