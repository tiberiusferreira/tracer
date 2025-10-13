use crate::recording::global_recorder::{
    clear_current_execution, get_global_recorder, set_current_execution,
};
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use uuid::Uuid;

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
            get_global_recorder().record_execution_end(*this.execution_context_id);
        }
    }
}

struct ExecutionGuard(Uuid);

impl Drop for ExecutionGuard {
    fn drop(&mut self) {
        clear_current_execution(self.0);
    }
}

impl<T: Future> Future for TopLevelFut<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let fut = this.inner;
        set_current_execution(*this.execution_context_id);
        // this guard makes sure it gets cleared even if the future panics
        let _guard = ExecutionGuard(*this.execution_context_id);
        let res = fut.poll(cx);
        res
    }
}
