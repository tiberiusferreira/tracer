use chrono::Utc;
use serde::{Deserialize, Serialize};

pub mod database;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimeSeries {
    pub id: i32,
    pub name: String,
    pub look_back_window_seconds: i32,
    pub max_interval_without_data_seconds: i32,
    pub max_value_threshold: Option<i32>,
    pub min_value_threshold: Option<i32>,
    pub query: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimeSeriesWithChecks {
    pub id: i32,
    pub name: String,
    pub look_back_window_seconds: i32,
    pub max_interval_without_data_seconds: i32,
    pub max_value_threshold: Option<i32>,
    pub min_value_threshold: Option<i32>,
    pub query: String,
    pub alert_checks: Vec<AlertCheck>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertCheck {
    pub id: i32,
    pub alert_message: Option<String>,
    pub notification_sent: bool,
    pub created_at: chrono::DateTime<Utc>,
}
