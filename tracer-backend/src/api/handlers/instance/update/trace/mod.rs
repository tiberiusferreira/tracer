use crate::api::handlers::instance::update::trace::get_trace_and_open_spans::Output;
use crate::api::handlers::instance::update::trace::span::InstanceDbId;
use crate::api::handlers::instance::update::trace::TraceUpdateError::MissingRoot;
use crate::api::ApiError;
use api_structs::instance::update::{Span, TraceSnapshot};
use api_structs::{InstanceUpdateId, TraceCountId};
use deepsize::DeepSizeOf;
use edgedb_codegen::edgedb_query;
use http::StatusCode;
use serde::Deserialize;
use sqlx::{Postgres, Transaction};
use std::collections::{HashMap, HashSet};
use std::fmt::Formatter;
use std::panic::Location;
use std::str::FromStr;
use thiserror::Error;
use tracing::{error, info, instrument};
use tracked_error::{error_chain_to_pretty_formatted, EdgeDBError, SqlxError};
use valuable_derive::Valuable;

mod span;

#[derive(Debug, Clone, Valuable)]
pub enum HttpMethod {
    Options,
    Get,
    Post,
    Put,
    Delete,
    Head,
    Trace,
    Connect,
    Patch,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Options => "options",
            HttpMethod::Get => "get",
            HttpMethod::Post => "post",
            HttpMethod::Put => "put",
            HttpMethod::Delete => "delete",
            HttpMethod::Head => "head",
            HttpMethod::Trace => "trace",
            HttpMethod::Connect => "connect",
            HttpMethod::Patch => "patch",
        }
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}
impl FromStr for HttpMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "options" => Ok(Self::Options),
            "get" => Ok(Self::Get),
            "post" => Ok(Self::Post),
            "put" => Ok(Self::Put),
            "delete" => Ok(Self::Delete),
            "head" => Ok(Self::Head),
            "trace" => Ok(Self::Trace),
            "connect" => Ok(Self::Connect),
            "patch" => Ok(Self::Patch),
            x => Err(format!("Unknown HTTP method: {}", x)),
        }
    }
}

#[derive(Debug, Clone, Valuable)]
struct TraceCacheData {
    status_code: Option<u16>,
    http_path: Option<String>,
    http_method: Option<HttpMethod>,
    has_errors: bool,
    warning_count: u32,
    span_count: u32,
    event_count: u32,
    byte_count: u32,
}

edgedb_query!(
    insert_trace,
    "
 insert Trace{
  trace_count_id := <int64>$trace_count_id,
  service_instance_update := (select ServiceInstanceUpdate filter .id=<uuid>$instance_update_id)
}
"
);

edgedb_query!(
    get_trace_and_open_spans,
    "
select assert_single(Trace{
  id,
  trace_count_id,
  open_spans := (
    select Trace.spans {
      id,
      span_count_id,
      duration_nanos,
      name: {
        id,
        name
      },
      attributes: {
        name: {
          id,
          name
        }
      }
    } filter Trace.spans.is_closed = false
  )
} filter Trace.trace_count_id = <int64>$trace_count_id)
"
);

#[derive(Debug, Error)]
pub enum TraceUpdateError {
    #[error("EdgeDB error: {0}")]
    EdgeDB(#[from] edgedb_tokio::Error),
    #[error(
        "SpanNotLinkedToRoot error at {location}: name={name} span_count_id={span_count_id} parent_span_count_id={parent_span_count_id}"
    )]
    SpanNotLinkedToRoot {
        span_count_id: u64,
        parent_span_count_id: Option<u64>,
        name: String,
        location: &'static Location<'static>,
    },
    #[error(
        "MissingOpenSpanInUpdate error at {location}: name={name} span_count_id={span_count_id} "
    )]
    MissingOpenSpanInUpdate {
        span_count_id: i64,
        name: String,
        location: &'static Location<'static>,
    },
    #[error("Missing Root Span error at {location}")]
    MissingRoot {
        location: &'static Location<'static>,
    },
    #[error("Parent Span is closed, but Child Span {child_name} Parent Span {parent_name} is not at {location}")]
    ParentClosedChildOpen {
        parent_name: String,
        child_name: String,
        location: &'static Location<'static>,
    },
    #[error("Child Span {child_name} was created at {child_created_at} before Parent {parent_name} at {parent_created_at} at {location}")]
    ChildCreatedBeforeParent {
        parent_name: String,
        parent_created_at: u64,
        child_name: String,
        child_created_at: u64,
        location: &'static Location<'static>,
    },
    #[error("Child Span {child_name} has longer duration {child_duration} than Parent {parent_name} duration {parent_duration} at {location}")]
    ChildDurationLongerThanParent {
        parent_name: String,
        parent_duration: u64,
        child_name: String,
        child_duration: u64,
        location: &'static Location<'static>,
    },
}

impl From<TraceUpdateError> for ApiError {
    fn from(e: TraceUpdateError) -> Self {
        let e_as_str = error_chain_to_pretty_formatted(&e);
        error!(
            e_as_str,
            "error being converted from TraceUpdateError to ApiError"
        );
        match &e {
            TraceUpdateError::EdgeDB(_) => ApiError {
                code: StatusCode::INTERNAL_SERVER_ERROR,
                message: e_as_str,
            },
            TraceUpdateError::MissingOpenSpanInUpdate { .. } => ApiError {
                code: StatusCode::BAD_REQUEST,
                message: e_as_str,
            },
        }
    }
}

#[instrument(skip_all)]
fn validate_received_trace(trace_snapshot: &TraceSnapshot) -> Result<(), TraceUpdateError> {
    let mut visited_span_count_ids: HashSet<u64> = HashSet::new();
    let root_span = trace_snapshot.spans.get(&1).ok_or_else(|| MissingRoot {
        location: Location::caller(),
    })?;
    let mut spans_to_visit: Vec<&Span> = vec![root_span];
    loop {
        let Some(parent) = spans_to_visit.pop() else {
            info!("visited all spans");
            break;
        };
        let current_span_children = trace_snapshot
            .spans
            .values()
            .filter(|s| s.parent_id == Some(parent.id))
            .collect::<Vec<_>>();
        for c in &current_span_children {
            let child = *c;
            spans_to_visit.push(child);
            let parent_is_closed = parent.is_closed;
            let child_is_not_closed = !child.is_closed;
            if parent_is_closed && child_is_not_closed {
                return Err(TraceUpdateError::ParentClosedChildOpen {
                    parent_name: parent.name.clone(),
                    child_name: child.name.clone(),
                    location: Location::caller(),
                });
            }
            if child.created_at_timestamp < parent.created_at_timestamp {
                return Err(TraceUpdateError::ChildCreatedBeforeParent {
                    parent_name: parent.name.clone(),
                    parent_created_at: parent.created_at_timestamp,
                    child_name: child.name.clone(),
                    child_created_at: child.created_at_timestamp,
                    location: Location::caller(),
                });
            }
            if parent.duration < child.duration {
                return Err(TraceUpdateError::ChildDurationLongerThanParent {
                    parent_name: parent.name.clone(),
                    parent_duration: parent.duration,
                    child_name: child.name.clone(),
                    child_duration: child.duration,
                    location: Location::caller(),
                });
            }
        }
        visited_span_count_ids.insert(parent.id);
    }
    for s in trace_snapshot.spans.values() {
        if !visited_span_count_ids.contains(&s.id) {
            return Err(TraceUpdateError::SpanNotLinkedToRoot {
                span_count_id: s.id,
                parent_span_count_id: s.parent_id,
                name: s.name.clone(),
                location: Location::caller(),
            });
        }
    }
    Ok(())
}

/// Traces are composed of spans
/// Some invariants need to be held:
/// - Each Trace must have a single root Span
/// - If a Span is closed, all its children also are
/// - A child can't start before its parent or end after it
/// - A closed Span cant be update
/// - The only update a span can have is its duration and attribute additions
///
/// Example of a value "tree" of spans
///         Root
/// |-----------------------|
///    |C1|         |C2|
///   |--------||---------|
///      C1-1
///    |------|
///      C1-2
///    |------|
///
///
/// Previous:
///     Root
/// |-------
///    |C1|
///   |-----
///
/// New:
///     Root
/// |-----------------
///    |C1|  |C2|
///   |---|  |--|
///   |C1-1|
///    |-|
///
///
/// Are cycles possible?
/// Cycles may happen if two spans are each others parents, but that cant be because if that was the case
/// there would be no link to root
/// Make sure all received spans were visited, if not it means we had spans not linked to root, example:
/// A - parent = B
/// B - parent = C
/// C - parent = A
#[instrument(skip_all)]
pub async fn insert_or_update_trace(
    tx: &mut edgedb_tokio::Transaction,
    instance_update_id: uuid::Uuid,
    trace_snapshot: &TraceSnapshot,
) -> Result<(), TraceUpdateError> {
    info!(
        instance.update.id = instance_update_id.to_string(),
        trace.count_id = trace_snapshot.trace_count_id,
        "inserting new trace"
    );
    let existing_trace_and_open_spans = get_trace_and_open_spans::transaction(
        &mut *tx,
        &get_trace_and_open_spans::Input {
            trace_count_id: trace_snapshot.trace_count_id as i64,
        },
    )
    .await?;
    match existing_trace_and_open_spans {
        None => {
            info!("no existing trace, adding a new one");
            validate_received_trace(trace_snapshot)?;
        }
        Some(existing) => {
            // The received spans must contain all the open spans we have in the DB.
            // For each active span we got from the DB, starting from Root:
            // Update closed if needed, update duration and add any new attributes
            // For each of its children in start time order
            //  if parent was closed, check make sure its closed too and update duration and attributes
            // Make sure all received spans were visited, if not it means we had spans not linked to root, example:
            // A - parent = B
            // B - parent = C
            // C - parent = A
            let mut visited_span_count_ids: HashSet<u64> = HashSet::new();
            let mut updated_spans: Vec<UpdatedSpan> = Vec::new();
            struct UpdatedSpan {
                id: uuid::Uuid,
                new_duration_nanos: i64,
                new_attributes: HashMap<String, serde_json::Value>,
            }
            for existing_span in &existing.open_spans {
                // get match
                let Some(updated_version) = trace_snapshot
                    .spans
                    .get(&(existing_span.span_count_id as u64))
                else {
                    return Err(TraceUpdateError::MissingOpenSpanInUpdate {
                        span_count_id: existing_span.span_count_id,
                        name: existing_span.name.name.clone(),
                        location: Location::caller(),
                    });
                };
                if updated_version.duration < existing_span.duration_nanos as u64 {
                    // todo: error out, new duration is shorter than last
                } else {
                    for (attribute_name, value) in &updated_version.attributes {
                        if let Some(existing_value) = existing_span
                            .attributes
                            .iter()
                            .find(|e| &e.name.name == attribute_name)
                        {
                            // got new value for existing attribute
                        }
                    }
                    updated_spans.push(UpdatedSpan {
                        id: existing_span.id,
                        new_duration_nanos: updated_version.duration as i64,
                        new_attributes: updated_version.attributes.clone(),
                    });
                }
            }
        }
    }
    let trace_id = insert_trace::transaction(
        &mut *tx,
        &insert_trace::Input {
            trace_count_id: trace_snapshot.trace_count_id as i64,
            instance_update_id,
        },
    )
    .await?
    .id;
    /// Invariants to hold:
    ///
    /// - Each Trace must have a single root Span
    ///     -  Check the root we have in the DB is the same as the one we got
    /// - Span span_count_ids are sequential and have no gaps
    /// - If a Span is closed, all its children also are.
    ///     - We get all open spans from the DB and check that if a parent is closed, all its children also are
    /// - A child can't start before its parent or end after it
    ///     - We can check that each child being added has its start date equal or after the parent.
    ///       When closing, we always close a child without children or with all children closed and check
    ///       that all direct closed children have child.duration <= self.duration
    /// - A closed Span cant be update
    ///     - We don't even fetch closed spans from the DB, so this will cause an error
    /// - The only update a span can have is its duration (only increase) and attribute additions
    ///     - We load the attribute names of the span in the DB and its duration and check
    Ok(())
}
// #[instrument(skip_all)]
// pub async fn insert_or_update_trace(
//     tx: &mut edgedb_tokio::Transaction,
//     trace_id: InstanceDbId,
//     instance_update_id: uuid::Uuid,
//     trace_snapshot: &TraceSnapshot,
// ) -> Result<(), SqlxError> {
//     // Span Ids start from 0 and must be sequential
//     // We assume the data in the DB is valid
//     // Get the max span id for the trace.
//     // For each span from the snapshot
//     // If its below or equal the max span id, we need to diff and update it.
//     //  During the update we need t
//     // If its above max span id, we know it's a new one and can "just" insert
//     // --
//     // We also need to valid that the parent_id
//     let max_span_id =
//         get_max_span_id(&mut *tx, instance_db_id, trace_snapshot.trace_count_id).await?;
//     match max_span_id {
//         None => {
//             // No spans. A trace has at least one span (the root one), so the trace doesn't exist yet.
//             // no need for diffing, raw insert.
//             // todo: check parent id are valid
//             // todo: check closed parent has all its children closed
//             insert_trace_header(
//                 &mut *tx,
//                 instance_db_id,
//                 instance_update_id,
//                 trace_snapshot.trace_count_id,
//             )
//             .await?;
//
//             let root_span = trace_snapshot
//                 .spans
//                 .get(&api_structs::instance::update::ROOT_SPAN_COUNT_ID);
//
//             let mut status_code: Option<u16> = None;
//             let mut http_path: Option<String> = None;
//             let mut http_method: Option<HttpMethod> = None;
//             if let Some(root_span) = &root_span {
//                 status_code = root_span
//                     .attributes
//                     .get("http.response.status_code")
//                     .map(|s| s.as_u64().map(|e| u16::try_from(e).ok()))
//                     .flatten()
//                     .flatten();
//                 http_path = root_span
//                     .attributes
//                     .get("url.path")
//                     .map(|s| s.as_str().map(ToOwned::to_owned))
//                     .flatten();
//                 http_method = root_span
//                     .attributes
//                     .get("http.request.method")
//                     .map(|s| s.as_str().map(|e| HttpMethod::from_str(e).ok()))
//                     .flatten()
//                     .flatten();
//             }
//             let has_errors = trace_snapshot.has_errors();
//             let warning_count = trace_snapshot.warning_count();
//             let span_count = trace_snapshot.spans.len() as u32;
//             let event_count = trace_snapshot
//                 .spans
//                 .values()
//                 .map(|v| v.events.len() as u32)
//                 .sum::<u32>();
//             let byte_count = trace_snapshot.spans.deep_size_of() as u32;
//             let trace_cache_data = TraceCacheData {
//                 status_code,
//                 http_path,
//                 http_method,
//                 has_errors,
//                 warning_count,
//                 span_count,
//                 event_count,
//                 byte_count,
//             };
//
//             // insert a dummy cache entry, we later update it
//             insert_trace_cache(
//                 &mut *tx,
//                 instance_db_id,
//                 trace_snapshot.trace_count_id,
//                 &trace_cache_data,
//             )
//             .await?;
//             span::insert_spans_and_events(
//                 &mut *tx,
//                 instance_db_id,
//                 instance_update_id,
//                 trace_snapshot.trace_count_id,
//                 trace_snapshot.spans.values(),
//             )
//             .await?;
//         }
//         Some(max_span_id) => {}
//     }
//     Ok(())
// }

#[instrument(skip_all)]
pub async fn insert_trace_header(
    tx: &mut Transaction<'static, Postgres>,
    instance_db_id: InstanceDbId,
    instance_update_id: api_structs::InstanceUpdateId,
    trace_id: api_structs::TraceCountId,
) -> Result<(), SqlxError> {
    let trace_id = trace_id as i32;
    let instance_update_id = instance_update_id as i32;
    sqlx::query!(
        "insert into trace (instance_id, trace_id, instance_update_id) \
        values ($1, $2, $3);",
        instance_db_id,
        trace_id,
        instance_update_id
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn insert_trace_cache(
    tx: &mut Transaction<'static, Postgres>,
    instance_db_id: InstanceDbId,
    trace_id: api_structs::TraceCountId,
    trace_cache_data: &TraceCacheData,
) -> Result<(), SqlxError> {
    let trace_id = trace_id as i32;
    let status_code = trace_cache_data.status_code.map(|e| e as i32);
    let http_path = &trace_cache_data.http_path;
    let http_method = &trace_cache_data
        .http_method
        .as_ref()
        .map(HttpMethod::as_str);
    let has_errors = trace_cache_data.has_errors;
    let warning_count = trace_cache_data.warning_count as i32;
    let span_count = trace_cache_data.span_count as i32;
    let event_count = trace_cache_data.event_count as i32;
    let byte_count = trace_cache_data.byte_count as i32;
    sqlx::query!(
        "insert into trace_cache (
            instance_id,
            trace_id,
            http_method,
            http_path,
            http_status,
            stored_spans_count,
            stored_events_count,
            stored_bytes_count,
            warnings_count,
            has_errors
    ) \
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10);",
        instance_db_id,
        trace_id,
        http_method as &Option<&str>,
        http_path as &Option<String>,
        status_code,
        span_count,
        event_count,
        byte_count,
        warning_count,
        has_errors
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn get_max_span_id(
    tx: &mut Transaction<'static, Postgres>,
    instance_db_id: i32,
    trace_id: api_structs::TraceCountId,
) -> Result<Option<i32>, SqlxError> {
    let max_span_id = sqlx::query_scalar!(
        "select max(span.span_id) from span where instance_id=$1 and trace_id=$2;",
        instance_db_id,
        trace_id as i32
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(max_span_id)
}
