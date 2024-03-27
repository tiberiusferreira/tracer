use crate::ui::trace::chunk::TraceId;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::{DisplayFromStr, NoneAsEmptyString};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceGridResponse {
    pub rows: Vec<TraceGridRow>,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceGridRow {
    pub trace_id: TraceId,
    pub started_at: u64,
    pub top_level_span_name: String,
    pub duration_ns: Option<u64>,
    pub spans_produced: u64,
    pub spans_stored: u64,
    pub events_produced: u64,
    pub events_dropped_by_sampling: u64,
    pub events_stored: u64,
    pub size_bytes: u64,
    pub warnings: u32,
    pub has_errors: bool,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Autocomplete {
    pub service_names: Vec<String>,
    pub top_level_spans: Vec<String>,
    // pub spans: Vec<String>,
    // pub keys: Vec<String>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchFor {
    #[serde_as(as = "DisplayFromStr")]
    pub from_date_unix: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub to_date_unix: u64,
    pub service_name: String,
    pub top_level_span: String,
    pub min_duration: u64,
    #[serde_as(as = "NoneAsEmptyString")]
    pub max_duration: Option<u64>,
    pub min_warns: u32,
    // pub key: String,
    // pub value: String,
    pub only_errors: bool,
}

impl SearchFor {
    pub fn to_query_parameters(&self) -> [(&'static str, String); 8] {
        let parameters = [
            ("from_date_unix", self.from_date_unix.to_string()),
            ("to_date_unix", self.to_date_unix.to_string()),
            ("service_name", self.service_name.to_string()),
            ("top_level_span", self.top_level_span.to_string()),
            ("min_duration", self.min_duration.to_string()),
            (
                "max_duration",
                self.max_duration.map(|e| e.to_string()).unwrap_or_default(),
            ),
            ("min_warns", self.min_warns.to_string()),
            ("only_errors", self.only_errors.to_string()),
        ];
        parameters
    }
}
