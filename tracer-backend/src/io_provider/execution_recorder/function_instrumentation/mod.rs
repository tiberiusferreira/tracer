use crate::io_provider::execution_recorder::{
    DataCollector, clear_current_execution, get_current_execution, get_global_collector,
    set_current_execution,
};
use chrono::{DateTime, Utc};
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use uuid::Uuid;

impl DataCollector {
    pub fn new_function_call(&self, execution_id: Uuid, name: &str) -> u64 {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let parent_id = execution_context.call_stack.last().cloned();
        let our_id = execution_context.executed_functions.len() as u64;
        execution_context
            .executed_functions
            .push(ExecutingFunction {
                id: our_id,
                parent_id,
                name: name.to_string(),
                start: Utc::now(),
                end: None,
            });
        our_id
    }

    pub fn resume_function(&self, execution_id: Uuid, function_id: u64) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        execution_context.call_stack.push(function_id);
    }

    pub fn pause_function(&self, execution_id: Uuid, function_id: u64) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let last_executing = execution_context.call_stack.pop().unwrap();
        assert_eq!(last_executing, function_id);
    }

    pub fn end_function(&self, execution_id: Uuid, function_id: u64) {
        let mut exec_context_w_guard = self.executions.write().unwrap();
        let execution_context = exec_context_w_guard.get_mut(&execution_id).unwrap();
        let fun = execution_context
            .executed_functions
            .get_mut(function_id as usize)
            .unwrap();
        fun.end = Some(Utc::now());
    }
}

fn create_new_function_within_current_execution(name: &str) -> Option<u64> {
    let id = match get_current_execution() {
        Some(curr) => {
            let function_id = get_global_collector().new_function_call(curr, name);
            Some(function_id)
        }
        None => None,
    };
    id
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
                let Some(current_execution) = get_current_execution() else {
                    panic!("tried to end function {function_id} without execution context");
                };
                get_global_collector().end_function(current_execution, function_id);
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
                let exec = get_current_execution().expect("to exist if there is a function id");
                get_global_collector().resume_function(exec, function_id);
                let res = fut.poll(cx);
                get_global_collector().pause_function(exec, function_id);
                res
            }
        }
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
