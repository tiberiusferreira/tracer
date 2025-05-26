use crate::io_provider::execution_recorder::function_instrumentation::instrument_function_within_task;
#[tokio::test]
async fn w() {
    crate::io_provider::execution_recorder::GLOBAL_DATA_COLLECTOR
        .set(crate::io_provider::execution_recorder::DataCollector::new())
        .unwrap();
    let _output = crate::io_provider::execution_recorder::record_execution(
        (),
        |input| async { work_on_trace_id().await },
        false,
    )
    .await;
    println!(
        "{:#?}",
        crate::io_provider::execution_recorder::get_global_collector().get_all_pruning()
    );
}

#[function_timer::time]
async fn sample_fun() -> i32 {
    println!("fun body");
    sample_fun_2().await;
    0
}

#[function_timer::time]
async fn sample_fun_2() {
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    println!("fun body 2");
}

#[function_timer::time]
async fn work_on_trace_id() -> i32 {
    println!("work_on_trace_id fun");
    // let db = io_provider.database();
    // let a: i32 = db.query_one("select 1", HashMap::new()).await.unwrap();
    // println!("{}", a);
    // tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    // sample_fun().await;
    0
}
