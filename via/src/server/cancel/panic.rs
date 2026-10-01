use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::task::{Context, Poll};

use delegate::delegate;

use crate::server::cancel::NotifyOnce;

use super::CancellationToken;

struct PanicHandle(CancellationToken);

struct CatchUnwind<F> {
    future: F,
    handle: PanicHandle,
}

#[inline(always)]
pub(crate) fn catch_unwind<F>(
    future: F,
    token: CancellationToken,
) -> impl Future<Output = ()> + Send + 'static
where
    F: Future<Output = ()> + Send + 'static,
{
    CatchUnwind {
        future,
        handle: PanicHandle(token),
    }
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
                let token = this.handle.token();

                token.state.panic();
                token.notify.notify_waiters();

                Poll::Ready(())
            }
        }
    }
}

impl PanicHandle {
    delegate! {
        to self.0 {
            fn token(&self) -> &NotifyOnce;
        }
    }
}

impl From<CancellationToken> for PanicHandle {
    #[inline]
    fn from(handle: CancellationToken) -> Self {
        Self(handle)
    }
}
