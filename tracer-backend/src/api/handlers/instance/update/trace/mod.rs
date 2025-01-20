use crate::api::ApiError;
use api_structs::instance::update::{Span, TraceFragment};
use edgedb_codegen::edgedb_query;
use http::StatusCode;
use std::collections::{HashMap, HashSet};
use std::fmt::Formatter;
use std::panic::Location;
use std::str::FromStr;
use thiserror::Error;
use tracing::{debug, error, info, instrument};
use tracked_error::{error_chain_to_pretty_formatted, EdgeDBError};
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

edgedb_query!(
    insert_trace,
    "
 insert Trace{
  trace_count_id := <int64>$trace_count_id,
  service_instance_update := (select ServiceInstanceUpdate filter .id=<uuid>$instance_update_id)
}
"
);

#[derive(Debug, Clone)]
pub struct ExistingDbTrace {
    id: uuid::Uuid,
    #[allow(unused)]
    trace_count_id: u64,
    current_span_count_id: u64,
    open_spans: Vec<RunningSpan>,
}
#[derive(Debug, Clone)]
pub struct RunningSpan {
    id: uuid::Uuid,
    parent_id: Option<uuid::Uuid>,
    span_count_id: u64,
    started_at: u64,
    duration_nanos: u64,
    name: String,
    attributes_names: HashSet<String>,
}

#[derive(Error, Debug, Clone)]
pub enum InvalidDBData {
    #[error("InvalidDBData: Got NonU64FieldValueInDB at {location}. Value {value} for field {field_name}")]
    NonU64FieldValueInDB {
        value: String,
        field_name: String,
        location: &'static Location<'static>,
    },
    #[error("InvalidDBData: Inconsistent DB state. Trace {trace_id} without spans")]
    DbTraceWithoutSpans { trace_id: uuid::Uuid },
    #[error("InvalidDBData: Inconsistent DB state. Trace {trace_id} has duplicate attributes for name {attribute_name}"
    )]
    DuplicateAttributesForSameName {
        trace_id: uuid::Uuid,
        attribute_name: String,
    },
}
impl TryFrom<get_trace_and_open_spans::Output> for ExistingDbTrace {
    type Error = InvalidDBData;

    fn try_from(value: get_trace_and_open_spans::Output) -> Result<Self, Self::Error> {
        let open_spans = value
            .open_spans
            .into_iter()
            .map(|open_span| {
                Ok(RunningSpan {
                    id: open_span.id,
                    parent_id: open_span.parent.map(|s| s.id),
                    span_count_id: u64::try_from(open_span.span_count_id).map_err(|_e| {
                        InvalidDBData::NonU64FieldValueInDB {
                            value: open_span.span_count_id.to_string(),
                            field_name: "span_count_id".to_string(),
                            location: Location::caller(),
                        }
                    })?,
                    started_at: u64::try_from(open_span.started_at_nanos).map_err(|_e| {
                        InvalidDBData::NonU64FieldValueInDB {
                            value: open_span.span_count_id.to_string(),
                            field_name: "started_at".to_string(),
                            location: Location::caller(),
                        }
                    })?,
                    duration_nanos: u64::try_from(open_span.duration_nanos).map_err(|_e| {
                        InvalidDBData::NonU64FieldValueInDB {
                            value: open_span.span_count_id.to_string(),
                            field_name: "duration_nanos".to_string(),
                            location: Location::caller(),
                        }
                    })?,
                    name: open_span.span_name.to_string(),
                    attributes_names: {
                        let mut attributes_names = HashSet::new();
                        for name in &open_span.attributes_names {
                            if !attributes_names.insert(name.to_string()) {
                                return Err(InvalidDBData::DuplicateAttributesForSameName {
                                    trace_id: value.id,
                                    attribute_name: name.to_string(),
                                });
                            }
                        }
                        attributes_names
                    },
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ExistingDbTrace {
            id: value.id,
            trace_count_id: u64::try_from(value.trace_count_id).map_err(|_e| {
                InvalidDBData::NonU64FieldValueInDB {
                    value: value.trace_count_id.to_string(),
                    field_name: "trace_count_id".to_string(),
                    location: Location::caller(),
                }
            })?,
            current_span_count_id: {
                let current_span_count_id = value
                    .current_span_count_id
                    .ok_or_else(|| InvalidDBData::DbTraceWithoutSpans { trace_id: value.id })?;
                u64::try_from(current_span_count_id).map_err(|_e| {
                    InvalidDBData::NonU64FieldValueInDB {
                        value: current_span_count_id.to_string(),
                        field_name: "current_span_count_id".to_string(),
                        location: Location::caller(),
                    }
                })?
            },
            open_spans,
        })
    }
}
edgedb_query!(
    get_trace_and_open_spans,
    "
select  assert_single(Trace{
  id,
  trace_count_id,
  current_span_count_id := assert_single(max(.spans.span_count_id)),
  open_spans := (
    select Trace.spans {
      id,
      span_count_id,
      parent,
      started_at_nanos,
      duration_nanos,
      span_name := .name,
      attributes_names := .attributes.name
    } filter Trace.spans.has_ended = false
  )
} filter Trace.service_instance_update.service_instance.id=<uuid>$service_instance_id and Trace.trace_count_id = <int64>$trace_count_id )
"
);

#[derive(Debug, Error)]
pub enum TraceUpdateError {
    #[error("EdgeDB error: {0}")]
    EdgeDB(#[from] EdgeDBError),
    #[error("Root not open in DB, got update for closed trace. Trace {trace_id}")]
    UpdateForClosedTrace { trace_id: uuid::Uuid },
    #[error("NonContiguousSpanCountIdInUpdate: Trace {trace_id} span counts ids {previous_span_count_id} and {new_span_count_id}"
    )]
    NonContiguousSpanCountIdInUpdate {
        trace_id: uuid::Uuid,
        previous_span_count_id: u64,
        new_span_count_id: u64,
    },
    #[error("NonContiguousSpanCountIdInNewTrace: Trace {trace_count_id} span counts ids {previous_span_count_id} and {new_span_count_id}"
    )]
    NonContiguousSpanCountIdInNewTrace {
        trace_count_id: u64,
        previous_span_count_id: u64,
        new_span_count_id: u64,
    },
    #[error(
        "SpanNotLinkedToRoot error at {location}: name={name} span_count_id={span_count_id} parent_span_count_id={parent_span_count_id:?}"
    )]
    SpanNotLinkedToRoot {
        span_count_id: u64,
        parent_span_count_id: Option<u64>,
        name: String,
        location: &'static Location<'static>,
    },
    #[error("Missing Root Span error at {location}")]
    MissingRoot {
        location: &'static Location<'static>,
    },
    #[error("Invalid Db Data at {location}.")]
    InvalidDBData {
        #[source]
        source: InvalidDBData,
        location: &'static Location<'static>,
    },
    #[error(
        "Span had attribute set again at {location}. Name {attribute_name} new value {new_value}"
    )]
    AttributeValueSetAgain {
        attribute_name: String,
        new_value: String,
        location: &'static Location<'static>,
    },
    #[error("Got unexpected field for existing span at {location}. Field {field_name} was {old_value} and now is {new_value}"
    )]
    ForbiddenFieldChange {
        field_name: String,
        old_value: String,
        new_value: String,
        location: &'static Location<'static>,
    },
    #[error("ParentChildSpanValidationError at {location}.")]
    ParentChildSpanValidationError {
        #[source]
        source: ChildParentValidationError,
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
            TraceUpdateError::EdgeDB(_) | TraceUpdateError::InvalidDBData { .. } => ApiError {
                code: StatusCode::INTERNAL_SERVER_ERROR,
                message: e_as_str,
            },
            TraceUpdateError::AttributeValueSetAgain { .. }
            | TraceUpdateError::UpdateForClosedTrace { .. }
            | TraceUpdateError::ForbiddenFieldChange { .. }
            | TraceUpdateError::SpanNotLinkedToRoot { .. }
            | TraceUpdateError::NonContiguousSpanCountIdInUpdate { .. }
            | TraceUpdateError::NonContiguousSpanCountIdInNewTrace { .. }
            | TraceUpdateError::ParentChildSpanValidationError { .. }
            | TraceUpdateError::MissingRoot { .. } => ApiError {
                code: StatusCode::BAD_REQUEST,
                message: e_as_str,
            },
        }
    }
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
///
/// ```
///         Root
/// |-----------------------|
///    |C1|         |C2|
///   |--------||---------|
///      C1-1
///    |------|
///      C1-2
///    |------|
/// ```
///
/// Trace Updates are incremental to what was already sent and acknowledged before.
/// They contain all spans which were open last time, their new state and any new spans.
///
/// New spans are always children of either the root span or previous open spans.
/// Update example:
///
/// Previous State
/// ```
///     Root
/// |-------
///    |C1|
///   |-----
///
/// New State (Update)
///     Root
/// |--------------------
///     |C1|      |C2|
///   |-----|   |-----|
///   |C1-1|
///    |--|
///```
///
/// Are cycles possible? (No)
/// Cycles may happen if two spans are each others parents, but that cant be because if that was the case
/// there would be no link to root
/// Make sure all received spans were visited, if not it means we had spans not linked to root, example:
/// A - parent = B
/// B - parent = C
/// C - parent = A
///
#[instrument(skip_all)]
pub async fn insert_or_update_trace(
    tx: &mut edgedb_tokio::Transaction,
    service_instance_id: uuid::Uuid,
    instance_update_id: uuid::Uuid,
    trace_fragment: &TraceFragment,
) -> Result<(), TraceUpdateError> {
    info!(
        instance.update.id = instance_update_id.to_string(),
        trace.count_id = trace_fragment.trace_count_id,
        "inserting or updating trace"
    );
    let existing_trace_and_open_spans = get_trace_and_open_spans::transaction(
        &mut *tx,
        &get_trace_and_open_spans::Input {
            service_instance_id,
            trace_count_id: trace_fragment.trace_count_id as i64,
        },
    )
    .await
    .map_err(|e| EdgeDBError::from(e))?;
    match existing_trace_and_open_spans {
        None => {
            info!("no existing trace, adding a new one");
            let mut spans_to_upsert = validate_new_trace_spans_for_insertion(trace_fragment)?;
            let trace_id = insert_trace::transaction(
                &mut *tx,
                &insert_trace::Input {
                    trace_count_id: trace_fragment.trace_count_id as i64,
                    instance_update_id,
                },
            )
            .await
            .map_err(|e| EdgeDBError::from(e))?
            .id;
            info!(trace.id = trace_id.to_string(), "new trace");
            upsert_spans(&mut *tx, instance_update_id, trace_id, &mut spans_to_upsert).await?;
        }
        Some(existing) => {
            info!(
                trace.id = existing.id.to_string(),
                trace.trace_count_id = existing.trace_count_id,
                "existing trace found"
            );
            let existing = ExistingDbTrace::try_from(existing).map_err(|e| {
                TraceUpdateError::InvalidDBData {
                    source: e,
                    location: Location::caller(),
                }
            })?;
            let mut spans_to_upsert =
                validate_spans_for_insert_or_update(&existing, trace_fragment)?;
            upsert_spans(
                &mut *tx,
                instance_update_id,
                existing.id,
                &mut spans_to_upsert,
            )
            .await?;
        }
    };

    Ok(())
}

async fn upsert_spans(
    tx: &mut edgedb_tokio::Transaction,
    instance_update_id: uuid::Uuid,
    trace_id: uuid::Uuid,
    spans_to_upsert: &mut SpansToUpsert,
) -> Result<(), TraceUpdateError> {
    span::insert_spans_and_events(
        tx,
        instance_update_id,
        trace_id,
        &mut spans_to_upsert.spans_to_insert,
    )
    .await?;
    Ok(())
}

fn validate_update_for_span(
    old_span: &RunningSpan,
    new_span: &Span,
) -> Result<(), TraceUpdateError> {
    if old_span.name != new_span.name {
        return Err(TraceUpdateError::ForbiddenFieldChange {
            field_name: "name".to_string(),
            old_value: old_span.name.clone(),
            new_value: new_span.name.clone(),
            location: Location::caller(),
        });
    }
    if old_span.started_at != new_span.started_at_nanos {
        return Err(TraceUpdateError::ForbiddenFieldChange {
            field_name: "started_at".to_string(),
            old_value: old_span.started_at.to_string(),
            new_value: new_span.started_at_nanos.to_string(),
            location: Location::caller(),
        });
    }
    if old_span.duration_nanos <= new_span.duration_nanos {
        return Err(TraceUpdateError::ForbiddenFieldChange {
            field_name: "duration".to_string(),
            old_value: old_span.duration_nanos.to_string(),
            new_value: new_span.duration_nanos.to_string(),
            location: Location::caller(),
        });
    }
    for (attribute_name, value) in &new_span.attributes {
        if old_span.attributes_names.contains(attribute_name) {
            return Err(TraceUpdateError::AttributeValueSetAgain {
                attribute_name: attribute_name.to_string(),
                new_value: value.to_string(),
                location: Location::caller(),
            });
        }
    }
    Ok(())
}

struct SpansToUpsert {
    spans_to_insert: Vec<Span>,
    #[allow(unused)]
    spans_to_update: Vec<SpanToUpdate>,
}
fn validate_new_trace_spans_for_insertion(
    new_trace_fragment: &TraceFragment,
) -> Result<SpansToUpsert, TraceUpdateError> {
    let mut span_count_ids = vec![];
    for span in new_trace_fragment.spans.values() {
        span_count_ids.push(span.id);
    }
    check_count_ids_are_contiguous(span_count_ids.as_mut_slice()).map_err(|e| {
        TraceUpdateError::NonContiguousSpanCountIdInNewTrace {
            trace_count_id: new_trace_fragment.trace_count_id,
            previous_span_count_id: e.prev,
            new_span_count_id: e.next,
        }
    })?;

    let child_from_api =
        new_trace_fragment
            .spans
            .get(&1)
            .ok_or_else(|| TraceUpdateError::MissingRoot {
                location: Location::caller(),
            })?;
    let mut spans_to_check: Vec<ChildToCheckWithParent> = vec![ChildToCheckWithParent {
        parent_from_api: None,
        child_from_db: None,
        child_from_api,
    }];
    let mut spans_to_insert: Vec<&Span> = vec![];
    let mut spans_to_update: Vec<SpanToUpdate> = vec![];
    let existing_spans_in_db: Vec<RunningSpan> = vec![];
    while !spans_to_check.is_empty() {
        check_parent_children(
            &mut spans_to_check,
            &existing_spans_in_db,
            new_trace_fragment.spans.values(),
            &mut spans_to_insert,
            &mut spans_to_update,
        )?;
    }
    assert_eq!(spans_to_update.len(), 0);
    check_all_spans_were_inserted_or_updated(
        new_trace_fragment.spans.values(),
        &spans_to_insert,
        &spans_to_update,
    )?;
    Ok(SpansToUpsert {
        spans_to_insert: spans_to_insert.into_iter().cloned().collect(),
        spans_to_update,
    })
}
fn validate_spans_for_insert_or_update(
    existing_trace: &ExistingDbTrace,
    new_trace_fragment: &TraceFragment,
) -> Result<SpansToUpsert, TraceUpdateError> {
    let mut span_count_ids = vec![existing_trace.current_span_count_id];
    for span in new_trace_fragment.spans.values() {
        if existing_trace.current_span_count_id < span.id {
            span_count_ids.push(span.id);
        }
    }
    check_count_ids_are_contiguous(span_count_ids.as_mut_slice()).map_err(|e| {
        TraceUpdateError::NonContiguousSpanCountIdInUpdate {
            trace_id: existing_trace.id,
            previous_span_count_id: e.prev,
            new_span_count_id: e.next,
        }
    })?;

    // We update the spans by starting from the existing root and, for span:
    // - If it's a new one, validate and insert it.
    // - - Check all children are new, then add all children as children to insert
    // - If it's an existing, validate and update it
    // - - For each child check if it exists and if so add to list of children to insert, else add to list to update
    //
    //
    // Previous State
    // ```
    //     Root
    // |-------
    //    |C1|
    //   |-----
    //
    // New State (Update)
    //     Root
    // |--------------------
    //     |C1|      |C2|
    //   |-----|   |-----|
    //   |C1-1|
    //    |--|
    //```
    // We start from existing root.
    let child_from_db = existing_trace
        .open_spans
        .iter()
        .find(|e| e.span_count_id == 1)
        .ok_or_else(|| TraceUpdateError::UpdateForClosedTrace {
            trace_id: existing_trace.id,
        })?;
    let child_from_api =
        new_trace_fragment
            .spans
            .get(&1)
            .ok_or_else(|| TraceUpdateError::MissingRoot {
                location: Location::caller(),
            })?;
    let mut spans_to_check: Vec<ChildToCheckWithParent> = vec![ChildToCheckWithParent {
        parent_from_api: None,
        child_from_db: Some(child_from_db),
        child_from_api,
    }];
    let mut spans_to_insert: Vec<&Span> = vec![];
    let mut spans_to_update: Vec<SpanToUpdate> = vec![];
    while !spans_to_check.is_empty() {
        check_parent_children(
            &mut spans_to_check,
            &existing_trace.open_spans,
            new_trace_fragment.spans.values(),
            &mut spans_to_insert,
            &mut spans_to_update,
        )?;
    }
    check_all_spans_were_inserted_or_updated(
        new_trace_fragment.spans.values(),
        &spans_to_insert,
        &spans_to_update,
    )?;

    Ok(SpansToUpsert {
        spans_to_insert: spans_to_insert.into_iter().cloned().collect(),
        spans_to_update,
    })
}

fn check_all_spans_were_inserted_or_updated<'a>(
    spans_in_fragment: impl Iterator<Item = &'a Span>,
    spans_to_insert: &[&Span],
    spans_to_update: &[SpanToUpdate],
) -> Result<(), TraceUpdateError> {
    let mut inserted_or_updated_span_count_ids = HashSet::new();
    for s in spans_to_insert {
        assert!(inserted_or_updated_span_count_ids.insert(s.id));
    }
    for s in spans_to_update {
        assert!(inserted_or_updated_span_count_ids.insert(s.span_count_id));
    }
    for s in spans_in_fragment {
        if !inserted_or_updated_span_count_ids.contains(&s.id) {
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

struct ChildToCheckWithParent<'a> {
    parent_from_api: Option<&'a Span>,
    child_from_api: &'a Span,
    child_from_db: Option<&'a RunningSpan>,
}

fn check_parent_children<'a>(
    children_to_check: &mut Vec<ChildToCheckWithParent<'a>>,
    existing_spans: &'a [RunningSpan],
    new_spans: impl Iterator<Item = &'a Span>,
    spans_to_insert: &mut Vec<&'a Span>,
    spans_to_update: &mut Vec<SpanToUpdate>,
) -> Result<(), TraceUpdateError> {
    let Some(child_to_check_with_parent) = children_to_check.pop() else {
        return Ok(());
    };
    if let Some(parent_from_api) = child_to_check_with_parent.parent_from_api {
        validate_child_parent(parent_from_api, child_to_check_with_parent.child_from_api).map_err(
            |e| TraceUpdateError::ParentChildSpanValidationError {
                source: e,
                location: Location::caller(),
            },
        )?;
    }
    let new_children: Vec<&Span> = new_spans
        .filter(|e| e.parent_id == Some(child_to_check_with_parent.child_from_api.id))
        .collect();
    match child_to_check_with_parent.child_from_db {
        None => {
            spans_to_insert.push(child_to_check_with_parent.child_from_api);
            for c in new_children {
                children_to_check.push(ChildToCheckWithParent {
                    parent_from_api: Some(child_to_check_with_parent.child_from_api),
                    // if child has no parent in DB, its children wont have either
                    child_from_db: None,
                    child_from_api: c,
                });
            }
        }
        Some(child_as_in_db) => {
            validate_update_for_span(child_as_in_db, child_to_check_with_parent.child_from_api)?;
            spans_to_update.push(SpanToUpdate {
                id: child_as_in_db.id,
                span_count_id: child_as_in_db.span_count_id,
                duration_nanos: child_to_check_with_parent.child_from_api.duration_nanos,
                attributes_to_add: child_to_check_with_parent.child_from_api.attributes.clone(),
            });

            let existing_children_in_db: Vec<&RunningSpan> = existing_spans
                .iter()
                .filter(|e| e.parent_id == Some(child_as_in_db.id))
                .collect();
            for new_child in new_children {
                if let Some(existing) = existing_children_in_db
                    .iter()
                    .find(|existing_child| existing_child.span_count_id == new_child.id)
                {
                    children_to_check.push(ChildToCheckWithParent {
                        parent_from_api: Some(child_to_check_with_parent.child_from_api),
                        child_from_db: Some(existing),
                        child_from_api: new_child,
                    });
                } else {
                    children_to_check.push(ChildToCheckWithParent {
                        parent_from_api: Some(child_to_check_with_parent.child_from_api),
                        child_from_db: None,
                        child_from_api: new_child,
                    });
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Error)]
pub enum ChildParentValidationError {
    #[error("Parent span is closed, but child span is open at {location} - child_name={child_name} parent_name={parent_name}")]
    ParentClosedChildOpen {
        child_name: String,
        parent_name: String,
        location: &'static Location<'static>,
    },
    #[error("ChildCreatedBeforeParent was created at {child_created_at} before parent at {parent_created_at} at {location} - child_name={child_name} parent_name={parent_name}"
    )]
    ChildCreatedBeforeParent {
        child_name: String,
        parent_name: String,
        parent_created_at: u64,
        child_created_at: u64,
        location: &'static Location<'static>,
    },
    #[error("Child span has longer duration {child_duration} than parent duration {parent_duration} at {location} - child_name={child_name} parent_name={parent_name}"
    )]
    ChildDurationLongerThanParent {
        child_name: String,
        parent_name: String,
        parent_duration: u64,
        child_duration: u64,
        location: &'static Location<'static>,
    },
}
fn validate_child_parent(parent: &Span, child: &Span) -> Result<(), ChildParentValidationError> {
    let child_is_not_closed = !child.has_ended;
    let parent_is_closed = parent.has_ended;
    if parent_is_closed && child_is_not_closed {
        return Err(ChildParentValidationError::ParentClosedChildOpen {
            child_name: child.name.clone(),
            parent_name: parent.name.clone(),
            location: Location::caller(),
        });
    }
    if child.started_at_nanos < parent.started_at_nanos {
        return Err(ChildParentValidationError::ChildCreatedBeforeParent {
            child_name: child.name.clone(),
            parent_name: parent.name.clone(),
            parent_created_at: parent.started_at_nanos,
            child_created_at: child.started_at_nanos,
            location: Location::caller(),
        });
    }
    if parent.duration_nanos < child.duration_nanos {
        return Err(ChildParentValidationError::ChildDurationLongerThanParent {
            child_name: child.name.clone(),
            parent_name: parent.name.clone(),
            parent_duration: parent.duration_nanos,
            child_duration: child.duration_nanos,
            location: Location::caller(),
        });
    }
    Ok(())
}

#[allow(unused)]
struct SpanToUpdate {
    id: uuid::Uuid,
    span_count_id: u64,
    duration_nanos: u64,
    attributes_to_add: HashMap<String, serde_json::Value>,
}

struct NonContiguousId {
    prev: u64,
    next: u64,
}

#[instrument(skip_all)]
fn check_count_ids_are_contiguous(count_ids: &mut [u64]) -> Result<(), NonContiguousId> {
    count_ids.sort_unstable();
    let Some(mut prev) = count_ids.first().copied() else {
        debug!("empty count ids");
        return Ok(());
    };
    for new_id in count_ids.iter().skip(1) {
        if *new_id != prev + 1 {
            return Err(NonContiguousId {
                prev,
                next: *new_id,
            });
        }
        prev = *new_id;
    }
    Ok(())
}
