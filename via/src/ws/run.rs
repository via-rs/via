use futures_core::Stream;
use futures_sink::Sink;
use std::future::Future;
use std::marker::PhantomPinned;
use std::mem;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::WebSocketStream;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use super::error::rescue;
use super::io::UpgradedIo;
use super::{Channel, Message, Request};
use crate::Error;

pub struct RunTask<T, App> {
    run: Pin<Box<Run<T, App>>>,
}

enum IoState {
    Receive,
    Send(Message),
    Flush,
}

struct Facade {
    listener: Pin<Box<dyn Future<Output = super::Result> + Send>>,
    state: IoState,
    stream: *mut WebSocketStream<UpgradedIo>,
    rendezvous: Channel,
}

struct Run<T, App> {
    listener: Arc<T>,
    request: Request<App>,
    stream: WebSocketStream<UpgradedIo>,
    facade: Option<Facade>,
    _pin: PhantomPinned,
}

impl<T, App, Await> RunTask<T, App>
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Await: Future<Output = super::Result> + Send + 'static,
{
    pub(super) fn new(
        listener: Arc<T>,
        request: Request<App>,
        stream: WebSocketStream<UpgradedIo>,
    ) -> Self {
        Self {
            run: Box::pin(Run::new(listener, request, stream)),
        }
    }
}

impl<T, App, Await> Future for RunTask<T, App>
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Await: Future<Output = super::Result> + Send + 'static,
{
    type Output = Result<(), Error>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        self.run.as_mut().poll(context)
    }
}

unsafe impl Send for Facade {}

impl Drop for Facade {
    fn drop(&mut self) {
        // Null out the raw pointer to prevent accidental use.
        // The rest of the fields in self are dropped automatically.
        self.stream = std::ptr::null_mut();
    }
}

macro_rules! log {
    ($level:tt($indent:literal), $fmt:literal $($arg:tt)*) => {
        log!(
            concat!("{}", stringify!($level),"(via::ws): ", $fmt),
            " ".repeat($indent)
            $($arg)*
        )
    };
    ($($args:tt)+) => {
        if cfg!(debug_assertions) {
            eprintln!($($args)*);
        }
    };
}

impl Future for Facade {
    type Output = super::Result;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            // Safety:
            //
            // stream is a mutable pointer to the stream field of the `Run<_, _>`
            // struct. Run is structurally pinned in a `Pin<Box<_>>` as a field
            // of `RunTask`. The only way to construct `Run<_, _>` is with the
            // `RunTask::new` fn.
            let stream = unsafe { &mut *self.stream };

            match &mut self.state {
                IoState::Receive => {
                    log!(info(0), "state = receive");

                    // Confirm that the listener can receive the next message.
                    if self.rendezvous.has_capacity()? {
                        log!(info(2), "listener is ready for the next message.");

                        // Attempt to pull the next message out of the stream.
                        if let Poll::Ready(next) = Pin::new(stream).poll_next(cx).map_err(rescue)? {
                            let Some(received) = next else {
                                // The stream has ended. The web socket is closed.
                                return Poll::Ready(Ok(()));
                            };

                            // If send fails, the channel is disconnected.
                            self.rendezvous.try_send(received)?;
                            log!(info(4), "message received.");
                        }
                    } else {
                        log!(info(2), "listener has not received the previous message.");
                    }

                    // The listener will probably register an additional wake.
                    if self.listener.as_mut().poll(cx)?.is_ready() {
                        // The listener future is ready and did not error.
                        return Poll::Ready(Ok(()));
                    }

                    if let Some(sent) = self.rendezvous.try_recv()? {
                        log!(info(4), "listener sent a message");
                        self.state = IoState::Send(sent);
                    } else {
                        log!(info(4), "wake for the next message or listener progress.");
                        return Poll::Pending;
                    }
                }

                state @ IoState::Send(_) => {
                    log!(info(0), "state = send");

                    let mut sink = Pin::new(stream);

                    if sink.as_mut().poll_ready(cx).map_err(rescue)?.is_pending() {
                        log!(info(2), "waiting for i/o to become available for write.");
                        return Poll::Pending;
                    }

                    if let IoState::Send(message) = mem::replace(state, IoState::Flush) {
                        sink.as_mut().start_send(message).map_err(rescue)?;
                        log!(info(2), "write successful. transitiion to flush.");
                    } else {
                        // We are in an invalid state. This can be interpreted
                        // as a signal that the risk of re-entrance is elevated
                        // and we should close the socket.
                        return Poll::Ready(Ok(()));
                    }
                }

                IoState::Flush => {
                    log!(info(0), "state = flush");

                    if Pin::new(stream).poll_flush(cx).map_err(rescue)?.is_ready() {
                        log!(info(2), "flush complete. transition to receive.");
                        self.state = IoState::Receive;
                    } else {
                        log!(info(2), "waiting for flush to complete.");
                        return Poll::Pending;
                    }
                }
            }
        }
    }
}

impl<T, App, Await> Run<T, App>
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Await: Future<Output = super::Result> + Send + 'static,
{
    fn new(listener: Arc<T>, request: Request<App>, stream: WebSocketStream<UpgradedIo>) -> Self {
        Self {
            listener,
            request,
            stream,
            facade: None,
            _pin: PhantomPinned,
        }
    }

    #[inline(always)]
    fn reconnect(&mut self) -> &mut Facade {
        let (ours, theirs) = Channel::new();
        let request = self.request.clone();
        let facade = Facade {
            listener: Box::pin((self.listener)(theirs, request)),
            state: IoState::Receive,
            stream: &mut self.stream as *mut _,
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

impl<T, App> Drop for Run<T, App> {
    fn drop(&mut self) {
        if let Some(facade) = self.facade.take() {
            // Safety: Explicitly dropping facade, invalidates the raw pointer.
            drop(facade);
        }
    }
}

impl<T, App, Await> Future for Run<T, App>
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Await: Future<Output = super::Result> + Send + 'static,
{
    type Output = Result<(), Error>;

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        // Safety:
        //
        // Self can only be constructed in with RunTask. RunTask wraps self in
        // Pin<Box<_>>, which guarantees a stable memory heap address.
        //
        // Self is also PhantomPinned, preventing self from moving.
        let this = unsafe { self.get_unchecked_mut() };

        let future = match this.facade.as_mut() {
            Some(facade) => facade,
            None => this.reconnect(),
        };

        match Pin::new(future).poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(())),
            Poll::Ready(Err(ControlFlow::Break(error))) => {
                log!(error(2), "{}", &error);
                Poll::Ready(Err(error))
            }
            Poll::Ready(Err(ControlFlow::Continue(error))) => {
                log!(warn(2), "{}", &error);

                this.reconnect();
                context.waker().wake_by_ref();

                Poll::Pending
            }
        }
    }
}
