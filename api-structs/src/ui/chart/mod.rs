use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimeBucketChart {
    pub x_axis_label: String,
    pub y_axis_label: String,
    pub x_time_buckets: Vec<chrono::DateTime<Utc>>,
    pub y_data: Vec<SeriesData>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SeriesData {
    pub series_name: String,
    pub data: Vec<f64>,
}
