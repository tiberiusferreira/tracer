use crate::ui::trace::chunk::{SingleChunkTraceQuery, TraceChunkId, TraceId};
use crate::Severity;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct TraceEventSearch {
    #[serde(flatten)]
    pub chunk: SingleChunkTraceQuery,
    pub severity: Vec<Severity>,
    pub substring: Option<String>,
    pub key_0: Option<String>,
    pub value_0: Option<String>,
    pub key_1: Option<String>,
    pub value_1: Option<String>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct TraceEventSearchUrlEncoded {
    pub trace_event_search: String,
}
