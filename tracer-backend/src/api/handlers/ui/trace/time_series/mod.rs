use crate::api::state::AppState;
use crate::api::ApiError;
use api_structs::ui::trace::grid::SearchFor;
use api_structs::Endpoint;
use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::instrument;

#[instrument(level = "error", skip_all)]
pub async fn handler(
    State(_app_state): State<AppState>,
    _search_for: Query<SearchFor>,
) -> Result<
    Json<<api_structs::ui::trace::time_series::TraceSummary as Endpoint>::ResponseBody>,
    ApiError,
> {
    //     let start = api_structs::time_conversion::time_from_nanos(search_for.from_date_unix).and_utc();
    //     let end = api_structs::time_conversion::time_from_nanos(search_for.to_date_unix).and_utc();
    //     let data_points: Vec<crate::series::SeriesDataPoint> = sqlx::query_as(
    //         "SELECT trace.created_at as time,
    //        count(*)                                                as data,
    //        trace_cache.has_errors                                  as group_name
    // FROM trace
    //          inner join trace_cache
    //                     on trace_cache.instance_id = trace.instance_id
    //                         and trace_cache.trace_id = trace.trace_id
    // where trace.created_at > $1 and trace.created_at <$2
    // GROUP BY time, group_name
    // ORDER BY time desc;",
    //     )
    //     .bind(start)
    //     .bind(end)
    //     .fetch_all(&app_state.con)
    //     .await
    //     .map_err(SqlxError::from)?;
    //     let mut time_bucket_chart = api_structs::ui::chart::TimeBucketChart {
    //         x_axis_label: "time [90s]".to_string(),
    //         y_axis_label: "Traces".to_string(),
    //         x_time_buckets: vec![],
    //         y_data: vec![],
    //     };
    //
    //     for d in data_points {
    //         time_bucket_chart.x_time_buckets.push(d.time);
    //         // time_bucket_chart.y_data.push(SeriesData{
    //         //     series_name: "".to_string(),
    //         //     data: vec![],
    //         // })
    //         // for a  in d.series_id
    //     }
    //     Ok(Json(api_structs::ui::chart::TimeBucketChart {
    //         x_axis_label: "".to_string(),
    //         y_axis_label: "".to_string(),
    //         x_time_buckets: vec![],
    //         y_data: vec![],
    //     }))
    unimplemented!()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SeriesData2 {
    pub category_name: String,
    pub data: Vec<f64>,
}

#[allow(unused)]
async fn w(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    time_bucket_size_seconds: u32,
    required_categories: Vec<String>,
    data_points: &mut [crate::series::SeriesDataPoint2],
) {
    data_points.sort_by(|a, b| a.time.cmp(&b.time));
    data_points.reverse();
    let mut x_points = vec![];
    let start_min = start.minute() - start.minute() % 5;
    let mut current = start
        .with_second(0)
        .unwrap()
        .with_minute(start_min)
        .unwrap();
    let mut categories: HashSet<String> = data_points
        .iter()
        .map(|d| d.category_name.clone())
        .collect();
    categories.extend(required_categories);
    let categories = categories;
    let mut series_by_name: HashMap<String, SeriesData2> = HashMap::new();
    let data_points_index = 0;
    loop {
        x_points.push(current);
        while let Some(_a) = data_points.get(data_points_index) {}
        for c in &categories {
            let _series_entry =
                series_by_name
                    .entry(c.to_string())
                    .or_insert_with(|| SeriesData2 {
                        category_name: c.to_string(),
                        data: vec![],
                    });
            // series_entry.data.push()
        }

        current += Duration::seconds(time_bucket_size_seconds as i64);
        if current > end {
            break;
        }
    }
    let _time_bucket_chart = api_structs::ui::chart::TimeBucketChart {
        x_axis_label: "time [90s]".to_string(),
        y_axis_label: "Traces".to_string(),
        x_time_buckets: vec![],
        y_data: vec![],
    };
}
