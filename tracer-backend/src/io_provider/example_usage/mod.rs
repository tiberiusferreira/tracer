use crate::io_provider::ExecutionIoProvider;
use crate::io_provider::execution_recorder::function_instrumentation::instrument_function_within_task;
use crate::io_provider::execution_recorder::{
    DataCollector, ExecutionKind, GLOBAL_DATA_COLLECTOR, get_global_collector, record_execution,
};
use std::collections::HashMap;
#[tokio::test]
async fn w() {
    GLOBAL_DATA_COLLECTOR.set(DataCollector::new()).unwrap();
    let _output = record_execution("my execution", ExecutionKind::Other, (), |input| async {
        work_on_trace_id().await
    })
    .await;
    println!("{:#?}", get_global_collector().get_all());
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
async fn work_on_trace_id() -> i32 {
    println!("work_on_trace_id fun");
    // let db = io_provider.database();
    // let a: i32 = db.query_one("select 1", HashMap::new()).await.unwrap();
    // println!("{}", a);
    // tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    // sample_fun().await;
    0
}
