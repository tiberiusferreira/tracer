use crate::instance::update::Location;
use crate::ui::trace::spans::SingleChunkTraceQuery;
use crate::Severity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: u64,
    pub message: Option<String>,
    pub severity: Severity,
    pub key_values: HashMap<String, String>,
    pub location: Location,
}
