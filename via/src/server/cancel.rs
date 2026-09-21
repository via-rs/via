use hyper::server::conn::*;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, ready};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Notify, futures::OwnedNotified};

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
use hyper_util::rt::TokioExecutor;

use super::io::IoWithPermit;
use crate::app::ServiceAdapter;

pub trait GracefulShutdown {
    fn graceful_shutdown(self: Pin<&mut Self>);
}

#[derive(Clone)]
pub struct CancellationToken {
    token: NotifyOnce,
}

pub(super) struct NotifyAbort {
    notify: Arc<Notify>,
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub(super) struct RunUntilCancelled<F> {
    abort: bool,
    future: F,
    notified: OwnedNotified,
}

pub(super) struct RunUntilCancelledProject<'a, F> {
    abort: &'a mut bool,
    future: Pin<&'a mut F>,
    notified: Pin<&'a mut OwnedNotified>,
}

#[derive(Clone)]
struct NotifyOnce {
    notified: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancellationToken {
    pub fn new() -> Self {
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

    pub fn notify_abort(&self) -> NotifyAbort {
        NotifyAbort {
            notify: Arc::clone(&self.token.notify),
        }
    }

    pub async fn wait(&self) {
        let future = self.token.wait();

        if !self.token.notified() {
            future.await;
        }
    }
}

impl NotifyAbort {
    pub(super) fn run_until_cancelled<F>(self, future: F) -> RunUntilCancelled<F>
    where
        F: Future<Output = Result<(), hyper::Error>> + GracefulShutdown + Send + 'static,
    {
        let notified = self.notify.notified_owned();

        RunUntilCancelled {
            abort: false,
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

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
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

impl<F: Unpin> RunUntilCancelled<F> {
    #[inline]
    fn project(self: Pin<&mut Self>) -> RunUntilCancelledProject<'_, F> {
        let this = unsafe { self.get_unchecked_mut() };
        let abort = &mut this.abort;
        let future = Pin::new(&mut this.future);
        let notified = unsafe { Pin::new_unchecked(&mut this.notified) };

        RunUntilCancelledProject {
            abort,
            future,
            notified,
        }
    }
}

impl<F> Future for RunUntilCancelled<F>
where
    F: Future<Output = Result<(), hyper::Error>> + GracefulShutdown + Send + Unpin + 'static,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();

        if !*this.abort && this.notified.poll(context).is_ready() {
            this.future.as_mut().graceful_shutdown();
            *this.abort = true;
        }

        if let Err(ref error) = ready!(this.future.poll(context)) {
            log!(info(service = 0), "{}", error);
        }

        Poll::Ready(())
    }
}
