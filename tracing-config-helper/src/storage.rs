use api_structs::exporter::{SpanEvent, TraceSummary};
use std::collections::HashMap;
use tracing::Id;

#[derive(Debug)]
pub struct ActiveTraceStorage {
    /// Links spans to their Root span that contains them, this also gives us the current span count
    child_id_to_root_id: HashMap<Id, Id>,
    /// Allows us to get to the root span by id
    root_spans: HashMap<Id, RootSpan>,
    orphan_events: Vec<SpanEvent>,
}

#[derive(Debug, Clone)]
pub struct RootSpan {
    pub id: Id,
    pub name: String,
    pub start: u64,
    pub duration: u64,
    pub key_vals: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
    // we keep a list of the placeholder children here so we can remove
    // them from the span_id_to_root_id list later
    pub children: HashMap<Id, NonRootSpan>,
    pub partial: bool,
}

impl RootSpan {
    pub fn summary(&self) -> TraceSummary {
        TraceSummary {
            id: self.id.into_non_zero_u64(),
            name: self.name.clone(),
            duration: self.duration,
            spans: self.spans(),
            events: self.events(),
            partial: self.partial,
        }
    }
    pub fn spans(&self) -> usize {
        // 1 from root
        1 + self.children.len()
    }
    pub fn events(&self) -> usize {
        let self_events = self.events.len();
        let children_events = self
            .children
            .values()
            .fold(0, |acc, curr| curr.events.len().saturating_add(acc));
        self_events + children_events
    }
}

#[derive(Debug, Clone)]
pub struct NonRootSpan {
    pub id: Id,
    pub name: String,
    pub parent_id: Id,
    pub start: u64,
    pub duration: u64,
    pub key_vals: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
}

#[derive(Debug)]
pub enum Error {
    DuplicateSpanIdInsertionAttempt,
}

pub enum SpanOrRoot<'a> {
    Root(&'a RootSpan),
    Span(&'a NonRootSpan),
}

pub enum SpanOrRootMut<'a> {
    Trace(&'a mut RootSpan),
    Span(&'a mut NonRootSpan),
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Ok,
    HadMissingData,
}

/// Raw Trace Storage, without imposing any limits
pub trait RawTracerStorage {
    fn push_orphan_event(&mut self, event: SpanEvent);
    fn push_root_span(&mut self, id: Id, name: String) -> Result<(), Error>;
    fn try_push_child_span(
        &mut self,
        id: Id,
        name: String,
        parent_id: Id,
    ) -> Result<Outcome, Error>;
    fn try_push_event(&mut self, span_id: &Id, event: SpanEvent) -> Result<Outcome, Error>;
    fn is_root(&self, span_id: &Id) -> Option<bool>;
    fn get(&mut self, span_id: &Id) -> Option<SpanOrRoot>;
    fn get_mut(&mut self, span_id: &Id) -> Option<SpanOrRootMut>;
    fn get_root(&self, root_id: &Id) -> Option<&RootSpan>;
    fn get_root_mut(&mut self, root_id: &Id) -> Option<&mut RootSpan>;
    fn get_root_of(&self, span_id: &Id) -> Option<&RootSpan>;
    fn get_root_of_mut(&mut self, span_id: &Id) -> Option<&mut RootSpan>;
    fn get_child(&self, span_id: &Id) -> Option<&NonRootSpan>;
    fn get_child_mut(&mut self, span_id: &Id) -> Option<&mut NonRootSpan>;
    fn remove(&mut self, root_id: &Id) -> Option<RootSpan>;
    // Replace once we get Impl Trait in Trait Position
    fn root_spans(&self) -> std::collections::hash_map::Values<'_, Id, RootSpan>;
    fn trace_summary(&self) -> Vec<TraceSummary>;
    fn get_orphan_events_len(&self) -> usize;
    fn take_orphan_events(&mut self) -> Vec<SpanEvent>;
}

impl RawTracerStorage for ActiveTraceStorage {
    fn push_orphan_event(&mut self, event: SpanEvent) {
        self.orphan_events.push(event);
    }
    fn push_root_span(&mut self, id: Id, name: String) -> Result<(), Error> {
        self.check_id_is_unused(&id)?;
        self.root_spans.insert(
            id.clone(),
            RootSpan {
                id: id.clone(),
                name: name.to_string(),
                start: u64::try_from(chrono::Utc::now().timestamp_nanos())
                    .expect("timestamp to fix u64"),
                duration: 0,
                key_vals: HashMap::new(),
                events: vec![],
                partial: false,
                children: HashMap::new(),
            },
        );
        self.child_id_to_root_id.insert(id.clone(), id.clone());
        Ok(())
    }

    fn try_push_child_span(
        &mut self,
        id: Id,
        name: String,
        parent_id: Id,
    ) -> Result<Outcome, Error> {
        self.check_id_is_unused(&id)?;
        let root = match self.get_root_of_mut(&parent_id) {
            None => return Ok(Outcome::HadMissingData),
            Some(root) => root,
        };
        let now =
            u64::try_from(chrono::Utc::now().timestamp_nanos()).expect("timestamp to fix u64");
        root.children.insert(
            id.clone(),
            NonRootSpan {
                id: id.clone(),
                name,
                parent_id,
                start: now,
                duration: 0,
                key_vals: Default::default(),
                events: vec![],
            },
        );
        let root_id = root.id.clone();
        self.child_id_to_root_id.insert(id.clone(), root_id);
        Ok(Outcome::Ok)
    }

    fn try_push_event(&mut self, span_id: &Id, event: SpanEvent) -> Result<Outcome, Error> {
        let span_mut = match self.get_mut(span_id) {
            None => return Ok(Outcome::HadMissingData),
            Some(span_mut) => span_mut,
        };
        match span_mut {
            SpanOrRootMut::Trace(root) => {
                root.events.push(event);
            }
            SpanOrRootMut::Span(span) => {
                span.events.push(event);
            }
        }
        Ok(Outcome::Ok)
    }

    fn is_root(&self, span_id: &Id) -> Option<bool> {
        self.child_id_to_root_id
            .get(span_id)
            .map(|root_id| root_id == span_id)
    }

    fn get(&mut self, span_id: &Id) -> Option<SpanOrRoot> {
        if self.is_root(span_id)? {
            Some(SpanOrRoot::Root(
                self.root_spans
                    .get(span_id)
                    .expect("to exist, if is_root returns true"),
            ))
        } else {
            let root = self.get_root_of(span_id)?;
            let real_span = root
                .children
                .get(span_id)
                .expect("if mapping to root exists, it must exist either as span or placeholder");
            Some(SpanOrRoot::Span(real_span))
        }
    }

    fn get_mut(&mut self, span_id: &Id) -> Option<SpanOrRootMut> {
        if self.is_root(span_id)? {
            Some(SpanOrRootMut::Trace(
                self.get_root_mut(span_id)
                    .expect("to exist, if is_root returns true"),
            ))
        } else {
            let root = self.get_root_of_mut(span_id)?;
            let real_span = root.children.get_mut(span_id)?;
            Some(SpanOrRootMut::Span(real_span))
        }
    }

    fn get_root(&self, root_id: &Id) -> Option<&RootSpan> {
        self.root_spans.get(root_id)
    }
    fn get_root_mut(&mut self, root_id: &Id) -> Option<&mut RootSpan> {
        self.root_spans.get_mut(root_id)
    }
    fn get_root_of(&self, span_id: &Id) -> Option<&RootSpan> {
        let root_id = self.child_id_to_root_id.get(span_id)?;
        self.root_spans.get(root_id)
    }

    fn get_root_of_mut(&mut self, span_id: &Id) -> Option<&mut RootSpan> {
        let root_id = self.child_id_to_root_id.get(span_id)?;
        self.root_spans.get_mut(root_id)
    }

    fn get_child(&self, span_id: &Id) -> Option<&NonRootSpan> {
        let root = self.get_root_of(span_id)?;
        root.children.get(span_id)
    }

    fn get_child_mut(&mut self, span_id: &Id) -> Option<&mut NonRootSpan> {
        let root = self.get_root_of_mut(span_id)?;
        root.children.get_mut(span_id)
    }

    fn remove(&mut self, root_id: &Id) -> Option<RootSpan> {
        let root_span = self.root_spans.remove(root_id)?;
        for child_id in root_span.children.keys() {
            self.child_id_to_root_id
                .remove(child_id)
                .expect("child to exist if root had it");
        }
        Some(root_span)
    }

    fn root_spans(&self) -> std::collections::hash_map::Values<'_, Id, RootSpan> {
        self.root_spans.values()
    }

    fn trace_summary(&self) -> Vec<TraceSummary> {
        self.root_spans().map(|q| q.summary()).collect()
    }

    fn get_orphan_events_len(&self) -> usize {
        self.orphan_events.len()
    }

    fn take_orphan_events(&mut self) -> Vec<SpanEvent> {
        std::mem::take(&mut self.orphan_events)
    }
}

// Public API
impl ActiveTraceStorage {
    pub fn new() -> ActiveTraceStorage {
        Self {
            child_id_to_root_id: HashMap::new(),
            root_spans: HashMap::new(),
            orphan_events: vec![],
        }
    }
}

impl ActiveTraceStorage {
    fn check_id_is_unused(&self, id: &Id) -> Result<(), Error> {
        if self.child_id_to_root_id.get(id).is_some() {
            Err(Error::DuplicateSpanIdInsertionAttempt)
        } else {
            Ok(())
        }
    }
}
