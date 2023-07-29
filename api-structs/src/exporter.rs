use std::collections::HashMap;
use std::num::NonZeroU64;
use std::str::FromStr;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Trace {
    pub service_name: String,
    pub id: NonZeroU64,
    pub name: String,
    pub start: u64,
    pub duration: u64,
    pub key_vals: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
    // we keep a list of the placeholder children here so we can remove
    // them from the span_id_to_root_id list later
    pub children: Vec<Span>,
    pub partial: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub id: NonZeroU64,
    pub name: String,
    pub parent_id: NonZeroU64,
    pub start: u64,
    pub duration: u64,
    pub key_vals: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: u64,
    pub level: Level,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceSummary {
    pub id: NonZeroU64,
    pub name: String,
    pub duration: u64,
    pub spans: usize,
    pub events: usize,
    pub partial: bool,
}
impl Trace {
    pub fn summary(&self) -> TraceSummary {
        TraceSummary {
            id: self.id,
            name: self.name.clone(),
            duration: self.duration,
            spans: self.spans(),
            events: self.events(),
            partial: self.partial,
        }
    }
    pub fn spans(&self) -> usize {
        // 1 from root
        1 + self.children.len()
    }
    pub fn events(&self) -> usize {
        let self_events = self.events.len();
        let children_events = self
            .children
            .iter()
            .fold(0, |acc, curr| curr.events.len().saturating_add(acc));
        self_events + children_events
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_lowercase_str(&self) -> &'static str {
        match self {
            Level::Trace => "trace",
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }
}

impl FromStr for Level {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "TRACE" => Ok(Self::Trace),
            "DEBUG" => Ok(Self::Debug),
            "INFO" => Ok(Self::Info),
            "WARN" => Ok(Self::Warn),
            "ERROR" => Ok(Self::Error),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SamplerLimits {
    pub span_plus_event_per_minute_per_trace_limit: u32,
    pub orphan_events_per_minute_limit: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub uuid: String,
    pub service_name: String,
    pub env: String,
    pub filters: TracerFilters,
    pub sampler_limits: SamplerLimits,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstanceStatus {
    pub config: Config,
    pub spe_dropped_on_export: u32,
    pub orphan_events_per_minute_usage: u32,
    pub orphan_events_per_minute_dropped: u32,
    pub trace_stats: HashMap<String, TraceStats>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceStats {
    pub warnings: usize,
    pub errors: usize,
    pub spe_usage_per_minute: u32,
    pub dropped_per_minute: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceApplicationStats {
    pub spe_usage_per_minute: u32,
    pub dropped_per_minute: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TracerStats {
    pub reconnects: u32,
    pub spe_dropped_on_export: u32,
    pub orphan_events_per_minute_usage: u32,
    pub orphan_events_per_minute_dropped: u32,
    pub per_minute_trace_stats: HashMap<String, TraceApplicationStats>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CollectorToApplicationMessage {
    Ack,
    Pong,
    GetConfig,
    GetStats,
    ChangeFilters(TracerFilters),
}
impl CollectorToApplicationMessage {
    pub fn is_ack(&self) -> bool {
        matches!(self, CollectorToApplicationMessage::Ack)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewSpan {
    pub id: NonZeroU64,
    pub trace_id: NonZeroU64,
    pub name: String,
    pub parent_id: Option<NonZeroU64>,
    pub start: u64,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewSpanEvent {
    pub span_id: NonZeroU64,
    pub name: String,
    pub timestamp: u64,
    pub level: Level,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewOrphanEvent {
    pub name: String,
    pub timestamp: u64,
    pub level: Level,
    pub key_vals: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClosedSpan {
    pub id: NonZeroU64,
    pub duration: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ApplicationToCollectorMessage {
    Ack,
    Ping,
    GetConfigResponse(Config),
    GetStatsResponse(TracerStats),
    ChangeFiltersResponse(Result<String, String>),
    NewSpan(NewSpan),
    NewSpanEvent(NewSpanEvent),
    ClosedSpan(ClosedSpan),
    NewOrphanEvent(NewOrphanEvent),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangeTracerFiltersRequest {
    pub uuid: String,
    pub new_trace_filters: TracerFilters,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TracerFilters {
    pub global: Level,
    pub per_crate: HashMap<String, Level>,
    pub per_span: HashMap<String, Level>,
}

impl TracerFilters {
    pub fn to_filter_str(&self) -> String {
        let per_crate = self
            .per_crate
            .iter()
            .map(|(crate_name, filter)| format!("{crate_name}={}", filter.as_lowercase_str()))
            .collect::<Vec<String>>()
            .join(",");
        let per_span = self
            .per_span
            .iter()
            .map(|(span_name, filter)| format!("[{span_name}]={}", filter.as_lowercase_str()))
            .collect::<Vec<String>>()
            .join(",");

        let non_empty: Vec<String> = vec![
            self.global.as_lowercase_str().to_string(),
            per_crate,
            per_span,
        ]
        .into_iter()
        .filter(|e| !e.is_empty())
        .collect();
        let filters = non_empty.join(",");
        filters
    }
}
