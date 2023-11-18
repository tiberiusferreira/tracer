use crate::Severity;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DisplayFromStr;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct SingleChunkTraceQuery {
    #[serde(flatten)]
    pub trace_id: TraceId,
    #[serde(flatten)]
    pub chunk_id: TraceChunkId,
}

#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct TraceId {
    #[serde_as(as = "DisplayFromStr")]
    pub service_id: i64,
    #[serde_as(as = "DisplayFromStr")]
    pub trace_id: i64,
}

#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct TraceChunkId {
    #[serde_as(as = "DisplayFromStr")]
    pub start_timestamp: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub end_timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub id: i64,
    pub timestamp: u64,
    pub parent_id: Option<i64>,
    pub duration: Option<u64>,
    pub name: String,
    pub events: Vec<Events>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Events {
    pub timestamp: u64,
    pub name: Option<String>,
    pub severity: Severity,
    // pub key_values: Vec<KeyValue>,
}

// #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
// pub struct KeyValue {
//     pub key: String,
//     pub user_generated: bool,
//     pub value: String,
// }
