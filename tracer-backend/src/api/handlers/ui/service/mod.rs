use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::ui::service::{
    DurationSummary, ExecutionSummary, RequestsSummary, SizeBytesSummary, Summaries,
};
use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::{max, max_by};
use std::collections::HashMap;
use std::ops::{AddAssign, DerefMut};

pub(crate) async fn data(
    State(app_state): State<AppState>,
    // Json(_new_filter): Json<api_structs::ui::service::NewFiltersRequest>,
) -> Result<Json<Summaries>, ApiError> {
    let db = app_state.execution_io_provider.database();
    let query = "select Execution{
  id,
  started_at,
  last_seen_at,
  size_bytes,
  status_code := (
    select .<execution[is Attributes]{
      _value
      }
    filter .name = 'status_code'
    limit 1
  )._value
}
  order by .started_at";
    let rollover_window_minutes = 5;
    let look_back_minutes = 180;
    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Execution {
        pub id: uuid::Uuid,
        pub started_at: chrono::DateTime<chrono::Utc>,
        pub last_seen_at: chrono::DateTime<chrono::Utc>,
        pub size_bytes: u64,
        pub status_code: Option<String>,
    }
    let mut summaries = Summaries {
        buckets: vec![],
        execution: ExecutionSummary {
            total: 0,
            with_warning_count: 0,
            with_errors_count: 0,
            values: vec![],
        },
        requests: RequestsSummary {
            total: 0,
            with_200_status_count: 0,
            with_non_200_status_count: 0,
            values: vec![],
        },
        size_bytes: SizeBytesSummary {
            total: 0,
            values: vec![],
        },
        duration: DurationSummary {
            max_ms: 0.0,
            max_values: vec![],
        },
    };
    let executions: Vec<Execution> = db.query(query, HashMap::from([])).await?;
    let end = Utc::now();
    let minutes_since_window_start = end.minute() % rollover_window_minutes;
    let end = end - chrono::Duration::minutes(minutes_since_window_start as i64);
    let end = end.with_nanosecond(0).unwrap().with_second(0).unwrap();
    let start = end - chrono::Duration::minutes(look_back_minutes as i64);
    let mut curr = start;

    while curr <= end {
        let bucket_start = curr;
        let bucket_end = curr + chrono::Duration::minutes(rollover_window_minutes as i64);
        summaries.buckets.push(curr);

        let executions_in_bucket: Vec<&Execution> = executions
            .iter()
            .filter(|e| e.started_at <= bucket_end && e.last_seen_at >= bucket_start)
            .collect();
        summaries.execution.total += executions_in_bucket.len() as u64;
        summaries
            .execution
            .values
            .push(executions_in_bucket.len() as f64);
        summaries.requests.values.push(0.);
        summaries.size_bytes.values.push(0.);
        summaries.duration.max_values.push(0.);
        for e in executions_in_bucket {
            if let Some(status_code) = &e.status_code {
                summaries.requests.total += 1;
                summaries.requests.values.last_mut().unwrap().add_assign(1.);
                if status_code == "200" {
                    summaries.requests.with_200_status_count += 1;
                } else {
                    summaries.requests.with_non_200_status_count += 1;
                }
                summaries
                    .size_bytes
                    .values
                    .last_mut()
                    .unwrap()
                    .add_assign(e.size_bytes as f64);
                summaries.size_bytes.total += e.size_bytes;
                let duration_ms = (e.last_seen_at - e.started_at).num_milliseconds() as f64;
                summaries.duration.max_ms =
                    max_by(duration_ms, summaries.duration.max_ms, |a, b| {
                        a.partial_cmp(b).unwrap()
                    });
                let last_value = *summaries.duration.max_values.last().unwrap();
                *summaries.duration.max_values.last_mut().unwrap() =
                    max_by(last_value, duration_ms, |a, b| a.partial_cmp(b).unwrap());
            }
        }

        // execution_summary.values.push(value as f64);
        curr = bucket_end;
    }

    Ok(Json(summaries))
}
