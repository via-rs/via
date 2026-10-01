mod panic;
mod state;

pub(crate) use panic::catch_unwind;

use delegate::delegate;
use hyper::server::conn::*;
use hyper_util::rt::TokioExecutor;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Notify;

use super::io::IoWithPermit;
use super::service::ServiceAdapter;

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
    value: Arc<NotifyOnce>,
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub(super) struct RunUntilCancelled<'a, F> {
    future: F,
    status: PollStatus,
    notify: Notified<'a>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PollStatus {
    Waiting,
    Proceed,
    Closing,
}

struct NotifyOnce {
    notify: Notify,
    state: CancellationState,
}

struct Notified<'a> {
    waiter: tokio::sync::futures::Notified<'a>,
    state: &'a CancellationState,
}

#[cfg_attr(not(debug_assertions), allow(unused_variables))]
fn on_ready(result: Result<(), hyper::Error>) {
    #[cfg(debug_assertions)]
    if let Err(error) = result {
        log!(error(service = 0), "{}", error);
    }
}

impl CancellationToken {
    pub(super) fn new() -> Self {
        let cancellation = Self {
            value: Arc::new(NotifyOnce {
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

                let token = cancellation.token();

                token.state.cancel();
                token.notify.notify_waiters();
            }
        });

        cancellation
    }

    pub(super) async fn wait(&self) -> bool {
        let waiter = self.token().notified();

        if self.token().is_waiting() {
            waiter.await;
        }

        self.token().did_panic()
    }

    pub(super) fn observe<F>(&self, future: F) -> RunUntilCancelled<'_, F>
    where
        F: Future<Output = Result<(), hyper::Error>> + GracefulShutdown + Send,
    {
        RunUntilCancelled {
            future,
            status: PollStatus::Waiting,
            notify: self.value.notified(),
        }
    }

    fn token(&self) -> &NotifyOnce {
        &self.value
    }
}

impl NotifyOnce {
    delegate! {
        to self.state {
            fn did_panic(&self) -> bool;
            fn is_waiting(&self) -> bool;
        }
    }

    fn notified(&self) -> Notified<'_> {
        Notified {
            state: &self.state,
            waiter: self.notify.notified(),
        }
    }
}

impl Future for Notified<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        // Safety: A pin projection.
        //
        // The `notify` field is never replaced or moved out of `self`. Also,
        // `self` is never replaced moved from the `RunUntilCancelled` future.
        let waiter = unsafe { self.map_unchecked_mut(|this| &mut this.waiter) };

        waiter.poll(context)
    }
}

impl<'a, F> RunUntilCancelled<'a, F> {
    delegate! {
        to self.notify.state {
            fn is_waiting(&self) -> bool;
        }
    }
}

impl<F> Future for RunUntilCancelled<'_, F>
where
    F: Future<Output = Result<(), hyper::Error>> + GracefulShutdown + Send + Unpin,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        // Safety: Futures are never replaced or moved out of `self`.
        let this = unsafe { self.get_unchecked_mut() };

        loop {
            match this.status {
                PollStatus::Waiting if this.is_waiting() => {
                    this.status = PollStatus::Proceed;
                }
                PollStatus::Closing => {
                    let future = Pin::new(&mut this.future);
                    return future.poll(context).map(on_ready);
                }
                status => {
                    let future = Pin::new(&mut this.future);

                    // Safety: A pin projection.
                    let notify = unsafe { Pin::new_unchecked(&mut this.notify) };

                    if notify.poll(context).is_ready() || status == PollStatus::Waiting {
                        future.graceful_shutdown();
                        this.status = PollStatus::Closing;
                    } else {
                        return future.poll(context).map(on_ready);
                    }
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
