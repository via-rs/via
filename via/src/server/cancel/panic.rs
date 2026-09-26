use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use super::{CancellationToken, NotifyOnce};

pub(crate) struct PanicHandle {
    notify: Arc<NotifyOnce>,
}

#[derive(Clone)]
pub(crate) struct UpgradeSupervisor {
    handle: Arc<Mutex<Option<PanicHandle>>>,
}

pub(super) struct CatchUnwind<F> {
    future: F,
    handle: PanicHandle,
}

#[inline(always)]
pub(crate) fn catch_unwind<F>(
    future: F,
    handle: PanicHandle,
) -> impl Future<Output = ()> + Send + 'static
where
    F: Future<Output = ()> + Send + 'static,
{
    CatchUnwind { future, handle }
}

impl<F> Future for CatchUnwind<F>
where
    F: Future<Output = ()> + Send + 'static,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let future = unsafe { Pin::new_unchecked(&mut this.future) };

        match std::panic::catch_unwind(AssertUnwindSafe(|| future.poll(context))) {
            Ok(output) => output,
            Err(_) => {
                this.handle.notify_panic();
                Poll::Ready(())
            }
        }
    }
}

impl PanicHandle {
    #[inline(always)]
    fn notify_panic(&self) {
        let token = &*self.notify;

        token.state.panic();
        token.notify.notify_waiters();
    }
}

impl From<CancellationToken> for PanicHandle {
    #[inline]
    fn from(token: CancellationToken) -> Self {
        Self {
            notify: token.value,
        }
    }
}

impl UpgradeSupervisor {
    #[inline(always)]
    pub(crate) fn new() -> Self {
        Self {
            handle: Default::default(),
        }
    }

    pub(crate) fn to_panic_handle(&self) -> Option<PanicHandle> {
        if let Ok(mut guard) = self.handle.try_lock() {
            guard.take()
        } else {
            None
        }
    }

    #[inline(always)]
    pub(super) fn set(&self, handle: PanicHandle) {
        if let Ok(mut guard) = self.handle.try_lock() {
            *guard = Some(handle);
        }
    }
}
