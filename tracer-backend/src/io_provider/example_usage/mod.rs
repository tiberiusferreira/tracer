use crate::io_provider::database::{
    DatabaseAccessor, ExecutionContext, ExecutionMetadata, ExecutionStarter, Parameter,
    instrument_function_within_task, track_task,
};
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::join;
use tracing::instrument;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
struct Trace {
    id: Uuid,
    trace_count_id: i32,
    created_at: DateTime<chrono::Utc>,
}

pub struct ExecutionWrapper {
    execution_context_id: u64,
}

pub struct Recorder {}
impl Recorder {}
struct ExecutionIoProvider {}

impl ExecutionWrapper {
    pub async fn new_execution<
        F: Future,
        Input: Serialize + DeserializeOwned,
        Fun: Fn(Input, ExecutionIoProvider) -> F,
    >(
        name: &str,
        input: Input,
        future_generator: Fun,
    ) -> <F as Future>::Output {
        let context = ExecutionContext::new(name);
        let execution_context_id = crate::io_provider::database::create_new_execution(context);
        let execution_io_provider = ExecutionIoProvider {};
        let future = future_generator(input, execution_io_provider);
        let res = track_task(future, execution_context_id).await;
        res
    }
}

#[tokio::test]
async fn w() {
    // let execution_starter = ExecutionStarter {
    //     client: gel_tokio::create_client().await.unwrap(),
    // };
    // let guard = execution_starter.start("test").await;
    // registers an execution context and until it gets dropped, all execution gets tied to it.
    // for sync functions, there can only be one at a time.
    // for async functions, the current context could have been created by another task that yielded.
    // we can't call async function with an execution_guard active
    let io_provider =
        ExecutionWrapper::new_execution("my execution", (), |input, runtime_io| async {
            work_on_trace_id(runtime_io).await
        })
        .await;
    // work_on_trace_id(&io_provider).await;
    // get IO from reference

    crate::io_provider::database::print_context();

    // sync functions get registered without issues because execution is linear
    // let database_accessor = execution.database_accessor();
    // async functions get polled at least once before they yield, on initial poll call they get correctly registered
    // let trace_id = insert_trace(database_accessor).await;
    // let trace_id = insert_trace(database_accessor).await;
    // work_on_trace_id(trace_id);
    // let w = get_trace_by_id(database_accessor, trace_id).await;
    // println!("{w:#?}");
    // EXECUTING_FUNCTIONS.with_borrow(|s| {
    //     println!("{:?}", s);
    // });
    // drop(guard);
}

#[my_macro::time]
async fn sample_fun() -> i32 {
    println!("fun body");
    sample_fun_2().await;
    0
}

#[my_macro::time]
async fn sample_fun_2() {
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    println!("fun body 2");
}

#[my_macro::time]
async fn work_on_trace_id(io_provider: ExecutionIoProvider) -> i32 {
    println!("work_on_trace_id fun");
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    sample_fun().await;
    0
}

pub async fn get_trace_by_id(database_accessor: &DatabaseAccessor, id: Uuid) -> Trace {
    let mut map = HashMap::new();
    map.insert(
        "id".to_string(),
        Parameter::Uuid {
            val: id,
            cast_to_table: None,
        },
    );
    let w: Trace = database_accessor
        .ro_query_one(
            "select Trace{id, created_at, trace_count_id} filter .id=<uuid>$id;",
            map,
        )
        .await
        .unwrap();
    w
}
pub async fn insert_trace(database_accessor: &DatabaseAccessor) -> Uuid {
    let mut map = HashMap::new();
    map.insert("trace_count_id".to_string(), Parameter::I32(1));
    map.insert(
        "service_instance".to_string(),
        Parameter::Uuid {
            val: Uuid::parse_str("b0c2364c-09f0-11f0-adcf-e7c4c0e0edfa").unwrap(),
            cast_to_table: Some("ServiceInstance".to_owned()),
        },
    );
    let id = database_accessor.insert("Trace", map).await.unwrap();
    id
}
