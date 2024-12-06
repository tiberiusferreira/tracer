use crate::api::handlers::instance::update::instance::database::InstanceDbId;
use api_structs::instance::update::TraceSnapshot;
use api_structs::InstanceUpdateId;
use deepsize::DeepSizeOf;
use serde::Deserialize;
use sqlx::{Postgres, Transaction};
use std::ascii::AsciiExt;
use std::fmt::Formatter;
use std::str::FromStr;
use tracing::instrument;
use tracked_error::SqlxError;
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

#[instrument(skip_all)]
pub async fn insert_or_update_trace(
    tx: &mut Transaction<'static, Postgres>,
    instance_db_id: InstanceDbId,
    instance_update_id: InstanceUpdateId,
    trace_snapshot: &TraceSnapshot,
) -> Result<(), SqlxError> {
    // Span Ids start from 0 and must be sequential
    // We assume the data in the DB is valid
    // Get the max span id for the trace.
    // For each span from the snapshot
    // If its below or equal the max span id, we need to diff and update it.
    //  During the update we need t
    // If its above max span id, we know it's a new one and can "just" insert
    // --
    // We also need to valid that the parent_id
    let max_span_id = get_max_span_id(&mut *tx, instance_db_id, trace_snapshot.trace_id).await?;
    match max_span_id {
        None => {
            // No spans. A trace has at least one span (the root one), so the trace doesn't exist yet.
            // no need for diffing, raw insert.
            // todo: check parent id are valid
            // todo: check closed parent has all its children closed
            insert_trace_header(
                &mut *tx,
                instance_db_id,
                instance_update_id,
                trace_snapshot.trace_id,
            )
            .await?;

            let root_span = trace_snapshot
                .spans
                .get(&api_structs::instance::update::ROOT_SPAN_ID);

            let mut status_code: Option<u16> = None;
            let mut http_path: Option<String> = None;
            let mut http_method: Option<HttpMethod> = None;
            if let Some(root_span) = &root_span {
                status_code = root_span
                    .attributes
                    .get("http.response.status_code")
                    .map(|s| s.as_u64().map(|e| u16::try_from(e).ok()))
                    .flatten()
                    .flatten();
                http_path = root_span
                    .attributes
                    .get("url.path")
                    .map(|s| s.as_str().map(ToOwned::to_owned))
                    .flatten();
                http_method = root_span
                    .attributes
                    .get("http.request.method")
                    .map(|s| s.as_str().map(|e| HttpMethod::from_str(e).ok()))
                    .flatten()
                    .flatten();
            }
            let has_errors = trace_snapshot.has_errors();
            let warning_count = trace_snapshot.warning_count();
            let span_count = trace_snapshot.spans.len() as u32;
            let event_count = trace_snapshot
                .spans
                .values()
                .map(|v| v.events.len() as u32)
                .sum::<u32>();
            let byte_count = trace_snapshot.spans.deep_size_of() as u32;
            let trace_cache_data = TraceCacheData {
                status_code,
                http_path,
                http_method,
                has_errors,
                warning_count,
                span_count,
                event_count,
                byte_count,
            };

            // insert a dummy cache entry, we later update it
            insert_trace_cache(
                &mut *tx,
                instance_db_id,
                instance_update_id,
                &trace_cache_data,
            )
            .await?;
            span::insert_spans_and_events(
                &mut *tx,
                instance_db_id,
                instance_update_id,
                trace_snapshot.trace_id,
                trace_snapshot.spans.values(),
            )
            .await?;
        }
        Some(max_span_id) => {}
    }
    Ok(())
}

#[instrument(skip_all)]
pub async fn insert_trace_header(
    tx: &mut Transaction<'static, Postgres>,
    instance_db_id: InstanceDbId,
    instance_update_id: api_structs::InstanceUpdateId,
    trace_id: api_structs::TraceId,
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
    trace_id: api_structs::TraceId,
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
    trace_id: api_structs::TraceId,
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
