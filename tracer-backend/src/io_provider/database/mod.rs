use chrono::{DateTime, NaiveDateTime, Utc};
use gel_errors::Error;
use gel_protocol::named_args;
use gel_protocol::value::Value;
use gel_protocol::value_opt::ValueOpt;
use pin_project_lite::pin_project;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::pin::{Pin, pin};
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use tracing::Instrument;
use tracing::instrument::Instrumented;
use tracked_error::error_chain_to_pretty_formatted;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseError {
    gel_result: String,
}

pub struct Database {
    client: gel_tokio::Client,
}

pub struct ExecutionStarter {
    pub client: gel_tokio::Client,
}

/// But the same thread can have multiple tasks executing, when re-entering a task we need to know
#[derive(Debug)]
pub struct ExecutionContext {
    execution_metadata: ExecutionMetadata,
    executed_functions: Vec<ExecutingFunction>,
    call_stack: Vec<u64>,
}

impl ExecutionContext {
    pub fn new(name: &str) -> ExecutionContext {
        Self {
            execution_metadata: ExecutionMetadata {
                started_at: Utc::now(),
                name: name.to_string(),
                warning_messages: Arc::new(Default::default()),
                error_messages: Arc::new(Default::default()),
                ended_at: None,
            },
            executed_functions: vec![],
            call_stack: vec![],
        }
    }
}

thread_local! {
    pub static THREAD_EXECUTIONS: RefCell<Vec<ExecutionContext >> = const { RefCell::new(Vec::new()) };
    pub static CURRENT_EXECUTION: Cell<Option<u64>> = const { Cell::new(None) };
}

pub fn print_context() {
    THREAD_EXECUTIONS.with(|executions| {
        let executions = &*executions.borrow();
        println!("{:#?}", executions);
        println!("{:#?}", CURRENT_EXECUTION.get());
    })
}

pub fn create_new_execution(context: ExecutionContext) -> u64 {
    assert_eq!(
        get_current_execution(),
        None,
        "there was already an execution in place"
    );
    let id = THREAD_EXECUTIONS.with_borrow_mut(|executions| {
        let id = executions.len();
        executions.push(context);
        id as u64
    });
    id
}
fn set_current_execution(id: u64) {
    println!("setting execution context as {id}");
    let old = CURRENT_EXECUTION.replace(Some(id));
    assert!(
        old.is_none(),
        "there was already some execution with id {old:#?}"
    );
}
fn get_current_execution() -> Option<u64> {
    let curr = CURRENT_EXECUTION.get();
    println!("returning current execution as {curr:#?}");
    curr
}

fn clear_current_execution(id: u64) {
    println!("removing {id} as execution context");
    let old = CURRENT_EXECUTION.replace(None);
    assert_eq!(old, Some(id), "execution didnt match {old:#?} {id}");
}

fn create_new_function_within_current_execution(name: &str) -> Option<u64> {
    let id = match get_current_execution() {
        Some(curr) => {
            let our_id = THREAD_EXECUTIONS.with_borrow_mut(|executions| {
                let current_execution = &mut executions[curr as usize];
                let parent_id = current_execution.call_stack.last().cloned();
                let our_id = current_execution.executed_functions.len() as u64;
                current_execution
                    .executed_functions
                    .push(ExecutingFunction {
                        id: our_id,
                        parent_id,
                        name: name.to_string(),
                        start: Utc::now(),
                        end: None,
                    });
                Some(our_id)
            });
            our_id
        }
        None => None,
    };
    id
}

fn end_function(function_id: u64) {
    println!("ending function {function_id}");
    let Some(current_execution) = get_current_execution() else {
        panic!("tried to end function {function_id} without execution context");
    };
    THREAD_EXECUTIONS.with_borrow_mut(|executions| {
        let current_execution = &mut executions[current_execution as usize];
        assert_eq!(
            current_execution.executed_functions[function_id as usize].end,
            None
        );
        current_execution.executed_functions[function_id as usize].end = Some(Utc::now());
    });
}
fn end_execution(id: u64) {
    println!("ending execution {id}");
    THREAD_EXECUTIONS.with_borrow_mut(|executions| {
        let current_execution = &mut executions[id as usize];
        assert_eq!(current_execution.execution_metadata.ended_at, None);
        current_execution.execution_metadata.ended_at = Some(Utc::now());
    });
}

fn resume_function(function_id: u64) {
    println!("resuming function {function_id}");
    let Some(current_execution) = get_current_execution() else {
        panic!("tried to resume function {function_id} without execution context");
    };
    THREAD_EXECUTIONS.with_borrow_mut(|executions| {
        let current_execution = &mut executions[current_execution as usize];
        current_execution.call_stack.push(function_id);
    });
}

fn pause_function(function_id: u64) {
    println!("pausing function {function_id}");
    let Some(current_execution) = get_current_execution() else {
        panic!("tried to pause function {function_id} without execution context");
    };
    THREAD_EXECUTIONS.with_borrow_mut(|executions| {
        let current_execution = &mut executions[current_execution as usize];
        let function_that_was_executing = current_execution.call_stack.pop();
        assert_eq!(function_that_was_executing, Some(function_id));
    });
}

pub fn track_task<F: Future>(fut: F, execution_context_id: u64) -> TopLevelFut<F> {
    TopLevelFut {
        inner: fut,
        execution_context_id,
    }
}

pin_project! {
    pub struct TopLevelFut<F> {
        #[pin]
        inner: F,
        execution_context_id: u64
    }
   impl<T> PinnedDrop for TopLevelFut<T> {
        fn drop(this: Pin<&mut Self>) {
            let this = this.project();
            end_execution(*this.execution_context_id);
        }
    }
}

impl<T: Future> Future for TopLevelFut<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let fut = this.inner;
        set_current_execution(*this.execution_context_id);
        let res = fut.poll(cx);
        clear_current_execution(*this.execution_context_id);
        res
    }
}

pin_project! {
    pub struct Fut<F> {
        #[pin]
        inner: F,
        function_id: Option<u64>,
    }
     impl<T> PinnedDrop for Fut<T> {
        fn drop(this: Pin<&mut Self>) {
            let this = this.project();
            if let Some(function_id) = *this.function_id {
                end_function(function_id)
            }
        }
    }
}

pub fn instrument_function_within_task<F: Future>(fut: F, name: &str) -> Fut<F> {
    let our_id = create_new_function_within_current_execution(name);
    Fut {
        inner: fut,
        function_id: our_id,
    }
}

impl<T: Future> Future for Fut<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let fut = this.inner;
        match *this.function_id {
            None => fut.poll(cx),
            Some(function_id) => {
                resume_function(function_id);
                let res = fut.poll(cx);
                pause_function(function_id);
                res
            }
        }
    }
}

impl ExecutionStarter {
    pub async fn start(self, description: &str) {
        //         let args = named_args! {
        //             "description" => description,
        //         };
        //         let id: uuid::Uuid = self
        //             .client
        //             .query_required_single(
        //                 "
        // select (insert Execution{
        //     description := <str>$description
        // }).id
        // ",
        //                 &args,
        //             )
        //             .await
        //             .unwrap();
        let execution = ExecutionMetadata {
            started_at: Utc::now(),
            // database_accessor: DatabaseAccessor {
            //     execution_id: id,
            //     client: self.client,
            // },
            ended_at: None,
            warning_messages: Arc::new(Default::default()),
            error_messages: Arc::new(Default::default()),
            name: "".to_string(),
        };
        let id = THREAD_EXECUTIONS.with_borrow_mut(|thread_executions| {
            let idx = thread_executions.len();
            let thread_execution_context = ExecutionContext {
                execution_metadata: execution,
                executed_functions: vec![],
                call_stack: vec![],
            };
            thread_executions.push(thread_execution_context);
            u64::try_from(idx).unwrap()
        });
        println!("Registered execution {description}");
    }
}

#[derive(Debug, Clone)]
pub struct ExecutingFunction {
    id: u64,
    parent_id: Option<u64>,
    name: String,
    start: DateTime<Utc>,
    end: Option<DateTime<Utc>>,
}

pub struct ExecutionMetadata {
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    warning_messages: Arc<RwLock<Vec<String>>>,
    error_messages: Arc<RwLock<Vec<String>>>,
    pub name: String,
}

impl Debug for ExecutionMetadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Execution")
            .field("name", &self.name)
            .field("started_at", &self.started_at)
            .field("warning_messages", &self.warning_messages.read().unwrap())
            .field("error_messages", &self.error_messages.read().unwrap())
            .finish()
    }
}

impl ExecutionMetadata {
    pub fn record_error(&self, error: &str) {
        self.error_messages.write().unwrap().push(error.to_string());
    }
    pub fn record_warning(&self, warning: &str) {
        self.warning_messages
            .write()
            .unwrap()
            .push(warning.to_string());
    }
    // pub fn database_accessor(&self) -> &DatabaseAccessor {
    //     &self.database_accessor
    // }
}

fn generate_insert_query(table: &str, columns: &HashMap<String, Parameter>) -> String {
    let mut column_set_queries = vec![];
    for (name, value) in columns {
        let bind_type = match value {
            Parameter::String(_) => "<str>".to_string(),
            Parameter::I32(_) => "<int32>".to_string(),
            Parameter::Uuid { cast_to_table, .. } => match cast_to_table {
                None => "<uuid>".to_string(),
                Some(cast_to_table) => {
                    format!("<{cast_to_table}><uuid>")
                }
            },
        };
        column_set_queries.push(format!("{name} := {bind_type}${name}"));
    }
    let column_set_query = column_set_queries.join(",\n");
    let query_str = format!(
        r#"select (insert {table}{{
    {column_set_query}
}}).id"#
    );
    query_str
}

#[derive(Debug)]
pub struct DatabaseAccessor {
    execution_id: Uuid,
    client: gel_tokio::Client,
}

impl DatabaseAccessor {
    pub async fn ro_query_one<T: Serialize + DeserializeOwned>(
        &self,
        query: &str,
        parameters: HashMap<String, Parameter>,
    ) -> Result<T, DatabaseError> {
        query_one_recording(&self.client, self.execution_id, query, parameters).await
    }
    pub async fn insert(
        &self,
        table: &str,
        columns: HashMap<String, Parameter>,
    ) -> Result<Uuid, DatabaseError> {
        let query = generate_insert_query(table, &columns);
        let entity_id: Uuid =
            query_one_recording(&self.client, self.execution_id, &query, columns.clone()).await?;
        let old: Option<serde_json::Value> = None;
        let mut new = serde_json::map::Map::new();
        for (k, v) in &columns {
            match v {
                Parameter::Uuid { val, .. } => {
                    new.insert(k.to_string(), serde_json::Value::String(val.to_string()));
                }
                Parameter::String(val) => {
                    new.insert(k.to_string(), serde_json::Value::String(val.to_string()));
                }
                Parameter::I32(val) => {
                    new.insert(
                        k.to_string(),
                        serde_json::Value::Number(serde_json::Number::from(*val)),
                    );
                }
            }
        }
        let new = serde_json::Value::Object(new);
        let new = Value::Json(gel_protocol::model::Json::new_unchecked(
            serde_json::to_string_pretty(&new).unwrap(),
        ));
        let args = named_args! {
              "entity_name" =>   table,
              "entity_id" =>   entity_id,
              "execution_id" =>   self.execution_id,
              "new" =>   new,
        };
        self.client
            .execute(
                "insert EntityChange{
    entity_name := <str>$entity_name,
    entity_id := <uuid>$entity_id,
    execution := <Execution><uuid>$execution_id,
    new := <json>$new
};",
                &args,
            )
            .await
            .unwrap();
        Ok(entity_id)
    }
}

impl Database {
    pub async fn new_execution(&self, description: &str) -> Result<QueryRecorder, Error> {
        let args = named_args! {
            "description" => description,
        };
        let id: uuid::Uuid = self
            .client
            .query_required_single(
                "
select (insert Execution{
    description := <str>$description
}).id
",
                &args,
            )
            .await
            .unwrap();
        Ok(QueryRecorder {
            client: self.client.clone(),
            id,
        })
    }
}

pub struct QueryRecorder {
    client: gel_tokio::Client,
    id: uuid::Uuid,
}
pub async fn record_query_start(
    client: &gel_tokio::Client,
    execution_id: Uuid,
    query_to_record: &str,
    parameters: &HashMap<String, Parameter>,
) -> Result<uuid::Uuid, DatabaseError> {
    let query = "select (insert ExecutionDatabaseQuery{
  execution := <Execution><uuid>$execution_id,
  query := <str>$query,
  parameters := <json>$parameters
}).id";
    let parameters = named_args! {
       "execution_id" => execution_id,
       "query" => query_to_record,
       "parameters" => Value::Json(gel_protocol::model::Json::new_unchecked(serde_json::to_string_pretty(&parameters).unwrap())),
    };
    let query_id: uuid::Uuid = client
        .query_required_single(query, &parameters)
        .await
        .unwrap();
    Ok(query_id)
}

pub async fn record_query_end(
    client: &gel_tokio::Client,
    query_id: uuid::Uuid,
    result: serde_json::Value,
) -> Result<(), DatabaseError> {
    let query = "update ExecutionDatabaseQuery
  filter .id = <uuid>$id
set {
  result := <json>$result,
  finished_at := datetime_of_statement()
}";
    let parameters = named_args! {
       "id" => query_id,
       "result" => Value::Json(gel_protocol::model::Json::new_unchecked(serde_json::to_string_pretty(&result).unwrap())),
    };
    client.execute(query, &parameters).await.unwrap();
    Ok(())
}
pub async fn query_one_recording<T: Serialize + DeserializeOwned>(
    client: &gel_tokio::Client,
    execution_id: Uuid,
    query: &str,
    parameters: HashMap<String, Parameter>,
) -> Result<T, DatabaseError> {
    let query_id = record_query_start(client, execution_id, query, &parameters).await?;
    let gel_params = params_to_gel(parameters);
    let gel_params: HashMap<&str, ValueOpt> = gel_params
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    let query_res: Result<gel_protocol::model::Json, _> = client
        .query_required_single_json(query, &gel_params)
        .await
        .map_err(|e| {
            let err_str = error_chain_to_pretty_formatted(&e);
            DatabaseError {
                gel_result: err_str,
            }
        });
    let query_res: Result<T, _> = query_res.map(|query| {
        serde_json::from_str(&query)
            .unwrap_or_else(|_e| panic!("failed to deserialize query from {query:#?})"))
    });
    let query_res_json = serde_json::to_value(&query_res).unwrap();
    record_query_end(client, query_id, query_res_json)
        .await
        .unwrap();
    query_res
}

fn params_to_gel(parameters: HashMap<String, Parameter>) -> HashMap<String, ValueOpt> {
    let mut hashmap = HashMap::new();
    for (k, v) in parameters {
        let a = match v {
            Parameter::String(v) => ValueOpt::from(Value::Str(v)),
            Parameter::I32(v) => ValueOpt::from(Value::Int32(v)),
            Parameter::Uuid { val, .. } => ValueOpt::from(val),
        };
        hashmap.insert(k, a);
    }
    hashmap
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Parameter {
    Uuid {
        val: uuid::Uuid,
        cast_to_table: Option<String>,
    },
    String(String),
    I32(i32),
}
