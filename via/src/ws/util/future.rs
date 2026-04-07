use futures_task::noop_waker;
use std::task::{Context, Poll};

/// Call the provided closure with a `&mut Context` that uses a noop waker.
pub fn poll_immediate_no_wake<T>(with: impl FnOnce(&mut Context) -> Poll<T>) -> Poll<T> {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    with(&mut cx)
}
