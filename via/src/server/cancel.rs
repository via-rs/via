use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

#[derive(Clone)]
pub struct Cancellation(Arc<Inner>);

pub struct Remote(Arc<Inner>);

struct Inner {
    cancelled: AtomicBool,
    notify: Notify,
}

impl Cancellation {
    pub fn new() -> (Self, Remote) {
        let inner = Arc::new(Inner {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        });

        (Self(Arc::clone(&inner)), Remote(inner))
    }

    pub async fn wait(&self) {
        let inner = &*self.0;

        if !inner.cancelled.load(Ordering::Acquire) {
            inner.notify.notified().await;
        }
    }
}

impl Remote {
    pub(super) fn cancel(&self) {
        let inner = &*self.0;

        inner.cancelled.store(true, Ordering::Release);
        inner.notify.notify_waiters();
    }
}
