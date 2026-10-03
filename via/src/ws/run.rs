use futures_core::Stream;
use futures_sink::Sink;
use std::future::Future;
use std::marker::PhantomPinned;
use std::mem::{self, ManuallyDrop};
use std::ops::ControlFlow;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::WebSocketStream;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use super::error::{into_break, is_restart, rescue};
use super::stream::WebSocketStreamMut;
use super::{Channel, Message, Request, upgrade::Listener};

pub struct RunTask<T, Io, App> {
    run: Pin<Box<Run<T, Io, App>>>,
}

enum IoState {
    Receive,
    Send(Message),
    Flush,
}

struct Facade<Io> {
    listener: Pin<Box<dyn Future<Output = super::Result> + Send>>,
    state: IoState,
    stream: WebSocketStreamMut<Io>,
    rendezvous: Channel,
}

struct Run<T, Io, App> {
    listener: Arc<Listener<T>>,
    request: Request<App>,
    stream: ManuallyDrop<WebSocketStream<Io>>,
    facade: Option<Facade<Io>>,
    _pin: PhantomPinned,
}

macro_rules! indent {
    ($i:ident = $value:expr) => {
        #[cfg(debug_assertions)]
        {
            $i = $value;
        }
    };
    ($i:ident) => {
        indent!($i = $i + 1);
    };
}

macro_rules! rescue_if {
    ($cond:expr, $error:expr) => {
        if $cond {
            return Poll::Ready(Err(rescue($error)));
        } else {
            return Poll::Ready(Err(ControlFlow::Break($error.into())));
        }
    };
}

impl<T, App, Io, Await> RunTask<T, Io, App>
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Io: AsyncRead + AsyncWrite + Send + Unpin,
    Await: Future<Output = super::Result> + Send + 'static,
{
    pub(super) fn new(
        listener: Arc<Listener<T>>,
        request: Request<App>,
        stream: WebSocketStream<Io>,
    ) -> Self {
        Self {
            run: Box::pin(Run {
                listener,
                request,
                stream: ManuallyDrop::new(stream),
                facade: None,
                _pin: PhantomPinned,
            }),
        }
    }
}

impl<T, App, Io, Await> Future for RunTask<T, Io, App>
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Io: AsyncRead + AsyncWrite + Send + Unpin,
    Await: Future<Output = super::Result> + Send + 'static,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        let this = self.get_mut();

        // Reification occurs as a result of projecting the `Pin<Box<Await>>`
        // stored in `self.run`.
        this.run.as_mut().poll(context)
    }
}

impl<Io> Future for Facade<Io>
where
    Io: AsyncRead + AsyncWrite + Send + Unpin,
{
    type Output = super::Result;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut restart = None;

        #[cfg(debug_assertions)]
        let mut i = 0;

        let this = self.get_mut();

        loop {
            match this.state {
                IoState::Receive => {
                    log!(info(ws = i), "state = receive");
                    indent!(i);

                    // Confirm that the listener can receive the next message.
                    //
                    // When `restart.is_some()`, this loop terminates before
                    // returning to `IoState::Receive`.
                    if this.rendezvous.has_capacity()? {
                        // Attempt to pull the next message out of the stream.
                        match Pin::new(&mut this.stream).poll_next(cx) {
                            Poll::Ready(Some(Ok(next))) => {
                                // If send fails, the channel is disconnected.
                                this.rendezvous.try_send(next)?;
                                log!(info(ws = i), "inbound message forwarded to listener.");
                            }
                            Poll::Ready(Some(Err(error))) => {
                                return Poll::Ready(Err(rescue(error)));
                            }
                            // The stream has ended. The web socket is closed.
                            Poll::Ready(None) => {
                                return Poll::Ready(Ok(()));
                            }
                            // The stream is empty. Poll the listener.
                            Poll::Pending => {}
                        }
                    } else {
                        log!(info(ws = i), "listener is busy.");
                    }

                    // The listener will probably register an additional wake.
                    if let Poll::Ready(result) = this.listener.as_mut().poll(cx) {
                        if let Err(error) = result {
                            if is_restart(&error) {
                                // Attempt to drain the channel before restart.
                                restart = Some(error);
                            } else {
                                return Poll::Ready(Err(error));
                            }
                        } else {
                            return Poll::Ready(Ok(()));
                        }
                    }

                    // A try_recv error is a disconnect.
                    if let Some(outbound) = this.rendezvous.try_recv()? {
                        this.state = IoState::Send(outbound);
                        log!(info(ws = i), "outbound message received from listener.");
                        indent!(i);
                    } else if let Some(op) = restart {
                        return Poll::Ready(Err(op));
                    } else {
                        log!(info(ws = i), "waiting for something interesting to happen.");
                        return Poll::Pending;
                    }
                }

                ref mut state @ IoState::Send(_) => {
                    log!(info(ws = i), "state = send");
                    indent!(i);

                    let IoState::Send(message) = mem::replace(state, IoState::Flush) else {
                        // We are in an invalid state. End the session.
                        return Poll::Ready(Ok(()));
                    };

                    match Pin::new(&mut this.stream).poll_ready(cx) {
                        Poll::Ready(Ok(_)) => {
                            if let Err(error) = Pin::new(&mut this.stream).start_send(message) {
                                rescue_if!(restart.is_none(), error);
                            } else {
                                log!(info(ws = i), "outbound message accepted by i/o.");
                                indent!(i);
                            }
                        }
                        Poll::Ready(Err(error)) => {
                            rescue_if!(restart.is_none(), error);
                        }
                        Poll::Pending => {
                            // If restart was requested, disconnect instead of buffering.
                            if let Some(op) = restart.map(into_break) {
                                return Poll::Ready(Err(op));
                            } else {
                                log!(info(ws = i), "waiting for i/o to become available.");
                                this.state = IoState::Send(message);
                                return Poll::Pending;
                            }
                        }
                    }
                }

                IoState::Flush => {
                    log!(info(ws = i), "state = flush");
                    indent!(i);

                    match Pin::new(&mut this.stream).poll_flush(cx) {
                        Poll::Ready(Ok(_)) => {
                            log!(info(ws = i), "outbound message sent successfully.");
                            if let Some(op) = restart {
                                return Poll::Ready(Err(op));
                            } else {
                                this.state = IoState::Receive;
                                cx.waker().wake_by_ref();
                                return Poll::Pending;
                            }
                        }
                        Poll::Pending => {
                            if let Some(op) = restart.map(into_break) {
                                return Poll::Ready(Err(op));
                            } else {
                                log!(info(ws = i), "waiting for flush to complete.");
                                return Poll::Pending;
                            }
                        }
                        Poll::Ready(Err(error)) => {
                            rescue_if!(restart.is_none(), error);
                        }
                    }
                }
            }
        }
    }
}

impl<T, App, Io, Await> Run<T, Io, App>
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Io: AsyncRead + AsyncWrite + Send + Unpin,
    Await: Future<Output = super::Result> + Send + 'static,
{
    #[inline(always)]
    fn reconnect(&mut self) -> &mut Facade<Io> {
        let (ours, theirs) = Channel::new();
        let request = self.request.clone();
        let facade = Facade {
            listener: Box::pin(((&*self.listener).handle)(theirs, request)),
            state: IoState::Receive,
            // Safety:
            //
            // Both `Facade` and `Run` uphold the invariants required to treat
            // this self-referential as `'static`. These types are not intended
            // for use outside of the context in which they are used.
            #[allow(clippy::explicit_auto_deref)]
            stream: unsafe { WebSocketStreamMut::new(&mut *self.stream) },
            rendezvous: ours,
        };

        self.facade = Some(facade);

        // Safety:
        //
        // We just assigned a `Some` value to self.facade.
        //
        // Implementing this any other way introduces an unlikely yet
        // recognizable re-entrancy pattern.
        unsafe { self.facade.as_mut().unwrap_unchecked() }
    }
}

impl<T, App, Io> Drop for Run<T, App, Io> {
    fn drop(&mut self) {
        // The `facade` field must be dropped before `stream`.
        if let Some(facade) = self.facade.take() {
            drop(facade);
        }

        // Safety:
        //
        // Manually dropping `stream` after `facade` upholds Rust's aliasing
        // rules of not more than one mutable borrow occuring at once.
        unsafe {
            ManuallyDrop::drop(&mut self.stream);
        }
    }
}

impl<T, App, Io, Await> Future for Run<T, Io, App>
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Io: AsyncRead + AsyncWrite + Send + Unpin,
    Await: Future<Output = super::Result> + Send + 'static,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        // Safety:
        //
        // `Self` is guaranteed a stable memory address by only occuring as a
        // boxed future. The visibility of `Self` is what upholds this
        // invariant.
        //
        // `Self` is never moved out of or replaced from it's original
        // allocation.
        let this = unsafe { self.get_unchecked_mut() };

        let future = match this.facade.as_mut() {
            Some(facade) => Pin::new(facade),
            None => Pin::new(this.reconnect()),
        };

        match future.poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(_)) => Poll::Ready(()),
            Poll::Ready(Err(ref op)) => {
                this.facade = None;
                match *op {
                    ControlFlow::Continue(ref error) => {
                        log!(error(ws = 0), "{}", error);
                        context.waker().wake_by_ref();
                        Poll::Pending
                    }
                    ControlFlow::Break(ref error) => {
                        log!(error(ws = 0), "{}", error);
                        Poll::Ready(())
                    }
                }
            }
        }
    }
}
