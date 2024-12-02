use chrono::Utc;
use serde::{Deserialize, Serialize};

pub mod database;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Series {
    pub id: i32,
    pub name: String,
    pub sql: String,
    pub check_window: i32,
    pub max_missing_data_points: Option<i32>,
    pub max_value_threshold: Option<i32>,
    pub min_value_threshold: Option<i32>,
    pub alert_checks: Vec<AlertCheck>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertCheck {
    pub id: i32,
    pub alert_message: Option<String>,
    pub notification_sent: bool,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct SeriesDataPoint {
    pub time: chrono::DateTime<Utc>,
    pub data: Option<i64>,
    pub series_id: Option<String>,
}
