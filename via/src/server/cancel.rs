use hyper::server::conn::*;
use hyper_util::rt::TokioExecutor;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Notify, futures::OwnedNotified};

use super::io::IoWithPermit;
use crate::app::ServiceAdapter;

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
    token: NotifyOnce,
}

pub(super) struct NotifyCancellation {
    notify: Arc<Notify>,
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub(super) struct RunUntilCancelled<F> {
    closing: bool,
    future: F,
    notified: OwnedNotified,
}

#[derive(Clone)]
struct NotifyOnce {
    notified: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancellationToken {
    pub(super) fn new() -> Self {
        let token = NotifyOnce {
            notified: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        };

        tokio::spawn({
            let token = token.clone();
            let ctrl_c = Box::pin(async {
                if tokio::signal::ctrl_c().await.is_err() {
                    eprintln!("unable to register the 'ctrl-c' signal.");
                }
            });

            async move {
                ctrl_c.await;
                token.notify();
            }
        });

        Self { token }
    }

    pub(super) fn notify_cancellation(&self) -> NotifyCancellation {
        NotifyCancellation {
            notify: Arc::clone(&self.token.notify),
        }
    }

    pub(super) async fn wait(&self) {
        let future = self.token.wait();

        if !self.token.notified() {
            future.await;
        }
    }
}

impl NotifyCancellation {
    pub(super) fn observe<F>(self, future: F) -> RunUntilCancelled<F>
    where
        F: Future<Output = Result<(), hyper::Error>> + GracefulShutdown + Send + 'static,
    {
        let notified = self.notify.notified_owned();

        RunUntilCancelled {
            closing: false,
            future,
            notified,
        }
    }
}

impl NotifyOnce {
    fn notified(&self) -> bool {
        self.notified.load(Ordering::Relaxed)
    }

    fn notify(&self) {
        self.notified.store(true, Ordering::Relaxed);
        self.notify.notify_waiters();
    }

    async fn wait(&self) {
        self.notify.notified().await
    }
}

impl<F> Future for RunUntilCancelled<F>
where
    F: Future<Output = Result<(), hyper::Error>> + GracefulShutdown + Send + Unpin + 'static,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        // Safety:
        //
        // The `notified` field of `self` implements `Future + !Unpin` and we
        // need to poll it to determine whether or not `F` should be cancelled.
        //
        // Data is never moved out of `self`. The only field that is directly
        // modified is the `closing` flag that we use to determine if the
        // `graceful_shutdown` method was called with `F`.
        let this = unsafe { self.get_unchecked_mut() };

        if !this.closing {
            // The size of the `Poll` returned from polling the `notified`
            // future is the size of a `bool`.
            //
            // It also represents two possible states, just like `bool`. Prefer
            // using a pattern so we can branch as-is.
            if let Poll::Ready(()) = Future::poll(
                // Safety: `notified` is never replaced or moved from `self`.
                unsafe { Pin::new_unchecked(&mut this.notified) },
                context,
            ) {
                Pin::new(&mut this.future).graceful_shutdown();
                this.closing = true;
            }
        }

        match Pin::new(&mut this.future).poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(_)) => Poll::Ready(()),
            Poll::Ready(Err(error)) => {
                log!(error(service = 0), "{}", error);
                Poll::Ready(())
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
