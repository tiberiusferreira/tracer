use crate::io_provider::execution_recorder::{
    DataCollector, clear_current_execution, get_current_execution, get_global_collector,
    set_current_execution,
};
use api_structs::instance::update::ExecutingFunction;
use chrono::Utc;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use uuid::Uuid;

impl DataCollector {
    pub fn new_function_call(
        &self,
        execution_id: Uuid,
        name: &str,
        module: &str,
        filename: &str,
        line: u32,
    ) -> u64 {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let parent_id = execution_context.current_call_stack.last().cloned();
        let our_id = execution_context.function_count;
        execution_context.function_count += 1;
        execution_context
            .executed_functions
            .push(ExecutingFunction {
                id: our_id,
                parent_id,
                name: name.to_string(),
                module: module.to_string(),
                filename: filename.to_string(),
                line,
                start: Utc::now(),
                end: None,
            });
        our_id
    }

    pub fn resume_function(&self, execution_id: Uuid, function_id: u64) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        execution_context.current_call_stack.push(function_id);
    }

    pub fn pause_function(&self, execution_id: Uuid, function_id: u64) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let last_executing = execution_context.current_call_stack.pop().unwrap();
        assert_eq!(last_executing, function_id);
    }

    pub fn end_function(&self, execution_id: Uuid, function_id: u64) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let fun = execution_context
            .executed_functions
            .iter_mut()
            .find(|f| f.id == function_id)
            .unwrap();
        fun.end = Some(Utc::now());
    }
}

fn create_new_function_within_current_execution(name: &str) -> Option<ExecutionAndFunctionId> {
    let execution_id = get_current_execution()?;
    let function_id =
        get_global_collector().new_function_call(execution_id, name, "module", "filename", 1);
    Some(ExecutionAndFunctionId {
        execution_id,
        function_id,
    })
}

pub fn track_task<F: Future>(fut: F, execution_context_id: Uuid) -> TopLevelFut<F> {
    TopLevelFut {
        inner: fut,
        execution_context_id,
    }
}

pin_project! {
    pub struct TopLevelFut<F> {
        #[pin]
        inner: F,
        execution_context_id: Uuid
    }
   impl<T> PinnedDrop for TopLevelFut<T> {
        fn drop(this: Pin<&mut Self>) {
            let this = this.project();
            get_global_collector().end_execution(*this.execution_context_id);
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

pub struct ExecutionAndFunctionId {
    pub execution_id: Uuid,
    pub function_id: u64,
}
pin_project! {
    pub struct Fut<F> {
        #[pin]
        inner: F,
        id: Option<ExecutionAndFunctionId>,
    }
     impl<T> PinnedDrop for Fut<T> {
        fn drop(this: Pin<&mut Self>) {
            let this = this.project();
            if let Some(id) = &this.id {
                get_global_collector().end_function(id.execution_id, id.function_id);
            }
        }
    }
}

pub fn instrument_function_within_task<F: Future>(fut: F, name: &str) -> Fut<F> {
    let id = create_new_function_within_current_execution(name);
    Fut { inner: fut, id }
}

impl<T: Future> Future for Fut<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let fut = this.inner;
        match &this.id {
            None => fut.poll(cx),
            Some(id) => {
                let exec = get_current_execution().expect("to exist if there is a function id");
                assert_eq!(exec, id.execution_id);
                get_global_collector().resume_function(id.execution_id, id.function_id);
                let res = fut.poll(cx);
                get_global_collector().pause_function(id.execution_id, id.function_id);
                res
            }
        }
    }
}
