use crate::ServiceId;
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServiceOrphanEventsRequest {
    #[serde(flatten)]
    pub service_id: ServiceId,
    pub from_date_unix: u64,
    pub to_date_unix: u64,
}
