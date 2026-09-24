mod state;

use hyper::server::conn::*;
use hyper_util::rt::TokioExecutor;
use std::mem::{self, ManuallyDrop};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Notify;
use tokio::sync::futures::Notified;

use super::io::IoWithPermit;
use crate::app::ServiceAdapter;
use state::CancellationState;

pub(super) trait GracefulShutdown {
    fn graceful_shutdown(self: Pin<&mut Self>);
}

/// A flat version of the `CancellationToken` from `tokio-util`.
//
// Via's router structure is tree-like and the JoinSet is a linked-list that
// resembles a tree when rotated.
//
// A third tree in the framework machinery starts to meaningfully widen the
// attack surface by holding multiple references originating from the main
// thread on which the accept fn receives incoming connections.
//
// Websockets are designed to tolerate abnormal closure where a TCP connection
// that sends request without a response is an error.
#[derive(Clone)]
pub(super) struct CancellationToken {
    token: Arc<NotifyOnce>,
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub(super) struct RunUntilCancelled<F> {
    status: PollStatus,
    future: F,
    notify: OwnedNotified,
}

#[derive(Clone, Copy, PartialEq)]
enum PollStatus {
    Waiting,
    Proceed,
    Closing,
}

struct NotifyOnce {
    notify: Notify,
    state: CancellationState,
}

struct OwnedNotified {
    token: Arc<NotifyOnce>,
    waiter: ManuallyDrop<Notified<'static>>,
}

impl CancellationToken {
    pub(super) fn new() -> Self {
        let cancellation = Self {
            token: Arc::new(NotifyOnce {
                notify: Notify::new(),
                state: CancellationState::new(),
            }),
        };

        tokio::spawn({
            let cancellation = cancellation.clone();
            let ctrl_c = Box::pin(async {
                if tokio::signal::ctrl_c().await.is_err() {
                    eprintln!("unable to register the 'ctrl-c' signal.");
                }
            });

            async move {
                ctrl_c.await;
                cancellation.notify();
            }
        });

        cancellation
    }

    pub(super) async fn wait(&self) -> bool {
        let waiter = self.token.notify.notified();

        if self.token.state.is_waiting() {
            waiter.await;
        }

        self.token.state.did_panic()
    }

    pub(super) fn observe<F>(self, future: F) -> RunUntilCancelled<F>
    where
        F: Future<Output = Result<(), hyper::Error>> + GracefulShutdown + Send + 'static,
    {
        let notify = self.token.notified_owned();

        RunUntilCancelled {
            status: PollStatus::Waiting,
            future,
            notify,
        }
    }
}

impl CancellationToken {
    fn notify(&self) {
        self.token.notify();
    }
}

impl NotifyOnce {
    fn notify(&self) {
        self.state.cancel();
        self.notify.notify_waiters();
    }

    fn notified_owned(self: Arc<Self>) -> OwnedNotified {
        let token = self;

        // Arc's pointee remains at the same address if the Arc handle moves.
        let notify: &Notify = &token.notify;

        // Safety:
        //
        // The `notify` field is retained and immutable for the waiter's
        // entire lifetime.
        //
        // The `waiter` field is dropped before `notify`, and neither field
        // is exposed for replacement or removal.
        let notify = unsafe { mem::transmute::<&Notify, &'static Notify>(notify) };

        // A borrowed waiter with a 'static lifetime.
        let waiter = { ManuallyDrop::new(notify.notified()) };

        OwnedNotified { token, waiter }
    }
}

impl OwnedNotified {
    #[inline]
    fn is_waiting(&self) -> bool {
        self.token.state.is_waiting()
    }

    fn panic(&self) {
        let token = self.token.as_ref();

        token.state.panic();
        token.notify.notify_waiters();
    }
}

impl Drop for OwnedNotified {
    fn drop(&mut self) {
        // Safety: Manually drop `waiter` to guarantee the correct drop order.
        unsafe { ManuallyDrop::drop(&mut self.waiter) };
    }
}

impl Future for OwnedNotified {
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        // Safety: A pin projection.
        //
        // The `notify` field is never replaced or moved out of `self`. Also,
        // `self` is never replaced moved from the `RunUntilCancelled` future.
        let waiter = unsafe { self.map_unchecked_mut(|this| &mut *this.waiter) };

        waiter.poll(context)
    }
}

impl<F> RunUntilCancelled<F>
where
    F: Future<Output = Result<(), hyper::Error>> + Unpin + 'static,
{
    fn poll_future(&mut self, context: &mut Context) -> Poll<()> {
        let future = Pin::new(&mut self.future);

        match catch_unwind(AssertUnwindSafe(|| future.poll(context))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(Ok(_))) => Poll::Ready(()),
            Ok(Poll::Ready(Err(error))) => {
                log!(error(service = 0), "{}", error);
                Poll::Ready(())
            }
            Err(_) => {
                self.notify.panic();
                Poll::Ready(())
            }
        }
    }
}

impl<F> Future for RunUntilCancelled<F>
where
    F: Future<Output = Result<(), hyper::Error>> + GracefulShutdown + Send + Unpin + 'static,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        // Safety: Futures are never replaced or moved out of `self`.
        let this = unsafe { self.get_unchecked_mut() };

        loop {
            match this.status {
                ref mut status @ PollStatus::Waiting => {
                    if this.notify.is_waiting() {
                        *status = PollStatus::Proceed;
                    } else {
                        Pin::new(&mut this.future).graceful_shutdown();
                        *status = PollStatus::Closing;
                    }
                }
                PollStatus::Proceed => {
                    // Safety: A pin projection.
                    let notify = unsafe { Pin::new_unchecked(&mut this.notify) };

                    if notify.poll(context).is_ready() {
                        Pin::new(&mut this.future).graceful_shutdown();
                        this.status = PollStatus::Closing;
                    }

                    return this.poll_future(context);
                }
                PollStatus::Closing => {
                    return this.poll_future(context);
                }
            }
        }
    }
}

impl<App, Io> GracefulShutdown
    for http1::UpgradeableConnection<IoWithPermit<Io>, ServiceAdapter<App>>
where
    App: Send + Sync + 'static,
    Io: AsyncRead + AsyncWrite + Unpin,
{
    #[inline]
    fn graceful_shutdown(self: Pin<&mut Self>) {
        http1::UpgradeableConnection::graceful_shutdown(self);
    }
}

impl<App, Io> GracefulShutdown
    for http2::Connection<IoWithPermit<Io>, ServiceAdapter<App>, TokioExecutor>
where
    App: Send + Sync + 'static,
    Io: AsyncRead + AsyncWrite + Unpin,
{
    #[inline]
    fn graceful_shutdown(self: Pin<&mut Self>) {
        http2::Connection::graceful_shutdown(self);
    }
}
