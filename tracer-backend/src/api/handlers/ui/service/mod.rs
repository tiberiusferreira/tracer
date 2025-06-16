use crate::api::ApiError;
use crate::api::state::AppState;
use api_structs::ui::service::{
    AttributeSummary, DurationSummary, EnvSummary, ExecutionHeader, ExecutionSummary,
    InstanceSummary, RequestsSummary, ServiceSummary, SizeBytesSummary, SummariesForGraph,
};
use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use chrono::{DateTime, Duration, Timelike, Utc};
use gel_io_recorder::Parameter;
use http::{StatusCode, header};
use serde::{Deserialize, Serialize};
use std::cmp::max_by;
use std::collections::HashMap;
use std::ops::AddAssign;
use uuid::Uuid;

fn attributes_filtering_statement(
    attributes: &HashMap<String, Option<String>>,
    params: &mut HashMap<String, Parameter>,
) -> String {
    let mut attribute_filter_stmt: Vec<String> = vec![];
    for (idx, (name, maybe_val)) in attributes.iter().enumerate() {
        let attribute_name = format!("attr_{}_name", idx);
        params.insert(attribute_name.clone(), Parameter::from(name));
        let filtering = match maybe_val {
            None => {
                format!("any(.attributes.name = <str>${attribute_name})")
            }
            Some(val) => {
                let attribute_val = format!("attr_{}_val", idx);
                params.insert(attribute_val.clone(), Parameter::from(val));
                format!(
                    "any(.attributes.name = <str>${attribute_name} and .attributes._value=<str>${attribute_val})"
                )
            }
        };
        attribute_filter_stmt.push(filtering);
    }
    let filter_stmt = if attribute_filter_stmt.is_empty() {
        "".to_string()
    } else {
        let filter = attribute_filter_stmt.join(" and ");
        format!(" and ({}) ", filter)
    };
    filter_stmt
}
pub(crate) async fn execution_list(
    State(app_state): State<AppState>,
    Json(filters): Json<api_structs::ui::service::ExecutionListFilters>,
) -> Result<Json<Vec<ExecutionHeader>>, ApiError> {
    let bucket = filters.bucket;
    let start = bucket;
    let end = bucket + Duration::minutes(5);
    let db = app_state.execution_io_provider.database();
    let mut params = HashMap::from([
        ("start_date".to_string(), Parameter::from(start)),
        ("end_date".to_string(), Parameter::from(end)),
    ]);
    let filter_stmt = attributes_filtering_statement(&filters.attributes, &mut params);
    let query = format!(
        "
select Execution{{
  external_id,
  service_name := .service_instance.service.name,
  started_at,
  duration_ms,
  size_bytes,
  status_code := (
    select .<execution[is ExecutionAttribute]{{
      _value
      }}
    filter .name = 'status_code'
    limit 1
  )._value,
  path := (
    select .<execution[is ExecutionAttribute]{{
      _value
      }}
    filter .name = 'uri'
    limit 1
  )._value,
  method := (
    select .<execution[is ExecutionAttribute]{{
      _value
      }}
    filter .name = 'method'
    limit 1
  )._value,
    attributes := (
    select .<execution[is ExecutionAttribute]{{
      name,
      _value
    }}
  )
}}
filter
    .started_at <= <datetime>$end_date and
    .last_seen_at >= <datetime>$start_date
    {filter_stmt}
  order by .started_at asc limit 200"
    );
    let executions: Vec<ExecutionHeader> = db.query(&query, params).await?;
    Ok(Json(executions))
}

#[derive(Deserialize)]
pub struct InstanceProfileQuery {
    pub instance_id: Uuid,
}
use base64::prelude::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct InstanceProfile {
    pub latest_profile_base64: Option<String>,
}
pub async fn instance_profile(
    State(app_state): State<AppState>,
    Query(query): Query<InstanceProfileQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let uuid = query.instance_id;
    let db = app_state.execution_io_provider.database();
    let mut tx = db.transaction_start().await;
    let instance_profile: Option<InstanceProfile> = tx
        .query_optional(
            "select ServiceInstance{
  latest_profile_base64
} filter .id=<uuid>$instance_id",
            HashMap::from([("instance_id".to_string(), Parameter::from(uuid))]),
        )
        .await?;
    let Some(instance_profile) = instance_profile else {
        return Err(ApiError {
            code: StatusCode::NOT_FOUND,
            message: "Service Instance not found".to_string(),
        });
    };
    match instance_profile.latest_profile_base64 {
        None => Err(ApiError {
            code: StatusCode::NOT_FOUND,
            message: "Service Instance has no profile".to_string(),
        }),
        Some(profile) => {
            let headers = axum::response::AppendHeaders([
                (header::CONTENT_TYPE, "image/svg+xml".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("filename=\"{uuid}-profile.svg\""),
                ),
            ]);
            let profile = STANDARD_NO_PAD.decode(&profile).unwrap();

            Ok((headers, profile))
        }
    }
}
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
    let end_rounded_to_window_end =
        end_rounded_to_window_start + Duration::minutes(rollover_window_minutes as i64);
    let minutes_since_start_window_start = start_datetime.minute() % rollover_window_minutes;
    let start_rounded_to_window_start =
        start_datetime - Duration::minutes(minutes_since_start_window_start as i64);
    let start_rounded_to_window_start = start_rounded_to_window_start
        .with_nanosecond(0)
        .unwrap()
        .with_second(0)
        .unwrap();

    let db = app_state.execution_io_provider.database();
    let mut params = HashMap::from([
        (
            "start_date".to_string(),
            Parameter::from(start_rounded_to_window_start),
        ),
        (
            "end_date".to_string(),
            Parameter::from(end_rounded_to_window_end),
        ),
    ]);
    let filter_stmt = attributes_filtering_statement(&filters.attributes, &mut params);
    let query = format!(
        "
select Execution{{
  id,
  service_env := .service_instance.service.env,
  service_name := .service_instance.service.name,
  instance_id := .service_instance.id,
  instance_created_at := .service_instance.registered_at,
  has_cpu_profile := exists .service_instance.latest_profile_base64,
  started_at,
  last_seen_at,
  size_bytes,
  status_code := (
    select .<execution[is ExecutionAttribute]{{
      _value
      }}
    filter .name = 'status_code'
    limit 1
  )._value,
  attributes := (
    select .<execution[is ExecutionAttribute]{{
      name,
      _value
    }}
  )
}}
  filter
    .started_at <= <datetime>$end_date and
    .last_seen_at >= <datetime>$start_date
    {filter_stmt}
  order by .started_at asc limit 10000"
    );

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Attribute {
        pub name: String,
        pub _value: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Execution {
        pub id: Uuid,
        pub service_env: String,
        pub service_name: String,
        pub instance_created_at: DateTime<Utc>,
        pub has_cpu_profile: bool,
        pub instance_id: Uuid,
        pub started_at: DateTime<Utc>,
        pub last_seen_at: DateTime<Utc>,
        pub size_bytes: u64,
        pub status_code: Option<String>,
        pub attributes: Vec<Attribute>,
    }

    let executions: Vec<Execution> = db.query(&query, params).await?;
    let mut attributes: HashMap<String, AttributeSummary> = HashMap::new();
    for e in &executions {
        for a in &e.attributes {
            let entry = attributes
                .entry(a.name.clone())
                .or_insert(AttributeSummary {
                    name: a.name.clone(),
                    count: 0,
                    values: HashMap::new(),
                });
            entry.count += 1;
            let value_count = entry.values.entry(a._value.clone()).or_insert(0);
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
        envs: HashMap::new(),
    };

    let mut curr = start_rounded_to_window_start;

    while curr <= end_rounded_to_window_start {
        let bucket_start = curr;
        let bucket_end = curr + Duration::minutes(rollover_window_minutes as i64);
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
            let env = summaries
                .envs
                .entry(e.service_env.clone())
                .or_insert(EnvSummary {
                    name: e.service_env.clone(),
                    execution_count: 0,
                    services: HashMap::new(),
                });
            env.execution_count += 1;
            let service = env
                .services
                .entry(e.service_name.clone())
                .or_insert(ServiceSummary {
                    name: e.service_name.clone(),
                    execution_count: 0,
                    instances: HashMap::new(),
                });
            service.execution_count += 1;
            let instance = service
                .instances
                .entry(e.instance_id)
                .or_insert(InstanceSummary {
                    instance_id: e.instance_id,
                    created_at: e.instance_created_at,
                    has_cpu_profile: e.has_cpu_profile,
                    execution_count: 0,
                });
            instance.execution_count += 1;
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
