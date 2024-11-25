use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DisplayFromStr;

use crate::instance::update::Location;
use crate::InstanceGlobalId;

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
    #[serde(flatten)]
    pub instance_id: InstanceGlobalId,
    #[serde_as(as = "DisplayFromStr")]
    pub trace_id: u32,
}

#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct TraceChunkId {
    // DisplayFromStr needed for using this as query parameter
    #[serde_as(as = "DisplayFromStr")]
    pub start_timestamp: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub end_timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub id: u32,
    pub timestamp: u64,
    pub parent_id: Option<u32>,
    pub duration: u64,
    pub name: String,
    pub key_values: HashMap<String, String>,
    pub location: Location,
}
