use crate::Endpoint;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub struct GetSeries;

impl Endpoint for GetSeries {
    const PATH: &'static str = "/api/ui/series";
    const METHOD: &'static str = "GET";
    type RequestBody = ();
    type QueryParameters = ();
    type ResponseBody = Vec<SeriesWithData>;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SeriesWithData {
    pub id: i32,
    pub name: String,
    pub sql: String,
    pub check_window: i32,
    pub max_missing_data_points: Option<i32>,
    pub max_value_threshold: Option<i32>,
    pub min_value_threshold: Option<i32>,
    pub data: Vec<SeriesDataPoint>,
    pub alert_checks: Vec<AlertCheck>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertCheck {
    pub result: AlertCheckResult,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum AlertCheckResult {
    Ok,
    AlertMessage(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SeriesDataPoint {
    pub time: chrono::DateTime<Utc>,
    pub data: Option<i64>,
    pub series_id: Option<String>,
}
