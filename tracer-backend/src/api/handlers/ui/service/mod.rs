use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::ui::service::{
    AttributeSummary, DurationSummary, ExecutionHeader, ExecutionSummary, RequestsSummary,
    SizeBytesSummary, SummariesForGraph,
};
use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Duration, Timelike, Utc};
use function_timer::time;
use gel_io_recorder::Parameter;
use serde::{Deserialize, Serialize};
use std::cmp::max_by;
use std::collections::{HashMap, HashSet};
use std::ops::AddAssign;
use tracing_config_helper::io_provider::execution_recorder::function_instrumentation::instrument_function_within_task;
pub(crate) async fn execution_list(
    State(app_state): State<AppState>,
    Json(filters): Json<api_structs::ui::service::ExecutionListFilters>,
) -> Result<Json<Vec<ExecutionHeader>>, ApiError> {
    let bucket = filters.bucket;
    let start = bucket;
    let end = bucket + Duration::minutes(5);
    let db = app_state.execution_io_provider.database();

    let query = "with
attr_name_1 := <optional str>$attr_name_1,
attr_value_1 := <optional str>$attr_val_1,
attr_name_2 := <optional str>$attr_name_2,
attr_value_2 := <optional str>$attr_val_2,
select Execution{
  id,
  service_name := .service_instance.service.name,
  started_at,
  duration_ms,
  size_bytes,
  status_code := (
    select .<execution[is ExecutionAttribute]{
      _value
      }
    filter .name = 'status_code'
    limit 1
  )._value,
  path := (
    select .<execution[is ExecutionAttribute]{
      _value
      }
    filter .name = 'uri'
    limit 1
  )._value,
  method := (
    select .<execution[is ExecutionAttribute]{
      _value
      }
    filter .name = 'method'
    limit 1
  )._value,
    matching_attributes := (
    select .<execution[is ExecutionAttribute]{
      name,
      _value
    } filter
      (
        not exists attr_name_1
        or (
          .name ?= attr_name_1
        and
          (not exists attr_value_1 or ._value ?= attr_value_1)
        )
      )
      and
      (
        not exists attr_name_2
        or (
          .name ?= attr_name_2
        and
          (not exists attr_value_2 or ._value ?= attr_value_2)
        )
      )
  )
}
filter
    .started_at <= <datetime>$end_date and
    .last_seen_at >= <datetime>$start_date
and (
      not exists (attr_name_1 union attr_name_2)
      or exists .matching_attributes
    )
  order by .started_at asc limit 100";

    let mut params = HashMap::from([
        ("start_date", Parameter::Datetime(start)),
        ("end_date", Parameter::Datetime(end)),
    ]);
    let mut attributes = filters.attributes.iter();
    let first = attributes.next();
    let second = attributes.next();
    match first {
        None => {
            params.insert("attr_name_1", Parameter::NoneString);
            params.insert("attr_val_1", Parameter::NoneString);
        }
        Some((k, v)) => {
            params.insert("attr_name_1", Parameter::String(k.to_string()));
            match v {
                None => {
                    params.insert("attr_val_1", Parameter::NoneString);
                }
                Some(v) => {
                    params.insert("attr_val_1", Parameter::String(v.to_string()));
                }
            }
        }
    }
    match second {
        None => {
            params.insert("attr_name_2", Parameter::NoneString);
            params.insert("attr_val_2", Parameter::NoneString);
        }
        Some((k, v)) => {
            params.insert("attr_name_2", Parameter::String(k.to_string()));
            match v {
                None => {
                    params.insert("attr_val_2", Parameter::NoneString);
                }
                Some(v) => {
                    params.insert("attr_val_2", Parameter::String(v.to_string()));
                }
            }
        }
    }
    let executions: Vec<ExecutionHeader> = db.query(query, params).await?;
    Ok(Json(executions))
}

#[time]
pub async fn summaries_for_graph(
    State(app_state): State<AppState>,
    Json(filters): Json<api_structs::ui::service::SummaryFilters>,
) -> Result<Json<SummariesForGraph>, ApiError> {
    let rollover_window_minutes = 5;
    let start_datetime = filters.start_date;
    let end_datetime = filters.end_date;
    let minutes_since_end_window_start = end_datetime.minute() % rollover_window_minutes;
    let end_rounded_to_window_start =
        end_datetime - Duration::minutes(minutes_since_end_window_start as i64);
    let end_rounded_to_window_start = end_rounded_to_window_start
        .with_nanosecond(0)
        .unwrap()
        .with_second(0)
        .unwrap();
    //
    let minutes_since_start_window_start = start_datetime.minute() % rollover_window_minutes;
    let start_rounded_to_window_start =
        start_datetime - Duration::minutes(minutes_since_start_window_start as i64);
    let start_rounded_to_window_start = start_rounded_to_window_start
        .with_nanosecond(0)
        .unwrap()
        .with_second(0)
        .unwrap();

    let db = app_state.execution_io_provider.database();
    let query = "with
attr_name_1 := <optional str>$attr_name_1,
attr_value_1 := <optional str>$attr_val_1,
attr_name_2 := <optional str>$attr_name_2,
attr_value_2 := <optional str>$attr_val_2,
select Execution{
  id,
  started_at,
  last_seen_at,
  size_bytes,
  status_code := (
    select .<execution[is ExecutionAttribute]{
      _value
      }
    filter .name = 'status_code'
    limit 1
  )._value,
  all_attributes := (
    select .<execution[is ExecutionAttribute]{
      name,
      _value
    }
  ),
  matching_attributes := (
    select .<execution[is ExecutionAttribute]{
      name,
      _value
    } filter
      (
        not exists attr_name_1
        or (
          .name ?= attr_name_1
        and
          (not exists attr_value_1 or ._value ?= attr_value_1)
        )
      )
      and
      (
        not exists attr_name_2
        or (
          .name ?= attr_name_2
        and
          (not exists attr_value_2 or ._value ?= attr_value_2)
        )
      )
  )
}
  filter
    .started_at <= <datetime>$end_date and
    .last_seen_at >= <datetime>$start_date
    and (
      not exists (attr_name_1 union attr_name_2)
      or exists .matching_attributes
    )
  order by .started_at asc limit 10000";

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Attribute {
        pub name: String,
        pub _value: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Execution {
        pub id: uuid::Uuid,
        pub started_at: DateTime<Utc>,
        pub last_seen_at: DateTime<Utc>,
        pub size_bytes: u64,
        pub status_code: Option<String>,
        pub all_attributes: Vec<Attribute>,
    }
    let mut params = HashMap::from([
        (
            "start_date",
            Parameter::Datetime(start_rounded_to_window_start),
        ),
        ("end_date", Parameter::Datetime(end_rounded_to_window_start)),
    ]);
    let mut attributes = filters.attributes.iter();
    let first = attributes.next();
    let second = attributes.next();
    match first {
        None => {
            params.insert("attr_name_1", Parameter::NoneString);
            params.insert("attr_val_1", Parameter::NoneString);
        }
        Some((k, v)) => {
            params.insert("attr_name_1", Parameter::String(k.to_string()));
            match v {
                None => {
                    params.insert("attr_val_1", Parameter::NoneString);
                }
                Some(v) => {
                    params.insert("attr_val_1", Parameter::String(v.to_string()));
                }
            }
        }
    }
    match second {
        None => {
            params.insert("attr_name_2", Parameter::NoneString);
            params.insert("attr_val_2", Parameter::NoneString);
        }
        Some((k, v)) => {
            params.insert("attr_name_2", Parameter::String(k.to_string()));
            match v {
                None => {
                    params.insert("attr_val_2", Parameter::NoneString);
                }
                Some(v) => {
                    params.insert("attr_val_2", Parameter::String(v.to_string()));
                }
            }
        }
    }
    let executions: Vec<Execution> = db.query(query, params).await?;
    let mut attributes: HashMap<String, AttributeSummary> = HashMap::new();
    for e in &executions {
        for a in &e.all_attributes {
            let entry = attributes
                .entry(a.name.clone())
                .or_insert(AttributeSummary {
                    name: a.name.clone(),
                    count: 0,
                    values: HashMap::new(),
                });
            entry.count += 1;
            let mut value_count = entry.values.entry(a._value.clone()).or_insert(0);
            *value_count += 1;
        }
    }
    let mut summaries = SummariesForGraph {
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
        attributes,
    };

    let mut curr = start_rounded_to_window_start;

    while curr <= end_rounded_to_window_start {
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
            }
            summaries
                .size_bytes
                .values
                .last_mut()
                .unwrap()
                .add_assign(e.size_bytes as f64);
            summaries.size_bytes.total += e.size_bytes;
            let duration_ms = (e.last_seen_at - e.started_at).num_milliseconds() as f64;
            summaries.duration.max_ms = max_by(duration_ms, summaries.duration.max_ms, |a, b| {
                a.partial_cmp(b).unwrap()
            });
            let last_value = *summaries.duration.max_values.last().unwrap();
            *summaries.duration.max_values.last_mut().unwrap() =
                max_by(last_value, duration_ms, |a, b| a.partial_cmp(b).unwrap());
        }
        curr = bucket_end;
    }

    Ok(Json(summaries))
}
