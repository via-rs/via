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
use super::{Channel, Message, Request};
use crate::Error;
use crate::server::IoStream;
use crate::ws::upgrade::Listener;

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
    stream: WebSocketStreamMut,
    rendezvous: Channel,
}

struct Run<T, App> {
    listener: Arc<Listener<T>>,
    request: Request<App>,
    stream: WebSocketStream<IoStream>,
    facade: Option<Facade>,
    _pin: PhantomPinned,
}

struct WebSocketStreamMut {
    io: *mut WebSocketStream<IoStream>,
}

impl<T, App, Await> RunTask<T, App>
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Await: Future<Output = super::Result> + Send + 'static,
{
    pub(super) fn new(
        listener: Arc<Listener<T>>,
        request: Request<App>,
        stream: WebSocketStream<IoStream>,
    ) -> Self {
        Self {
            run: Box::pin(Run {
                listener,
                request,
                stream,
                facade: None,
                _pin: PhantomPinned,
            }),
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

impl<T, App> Drop for Run<T, App> {
    fn drop(&mut self) {
        if let Some(facade) = self.facade.take() {
            // Safety: Explicitly dropping facade, invalidates the raw pointer.
            drop(facade);
        }
    }
}

impl Drop for Facade {
    fn drop(&mut self) {
        // Defensive poisoning. Dereferencing a null ptr and a dangling reference
        // to an I/O stream are both undefined behavior. However, dereferencing a
        // null ptr is inherently less risky on modern operating systems.
        //
        // Accessing a value after it has been dropped is impossible to do in
        // Safe Rust and none of the unsafe blocks found in this module allow
        // it to happen.
        //
        // Soundness relies on `Facade` being dropped before the `Run::stream`
        // field is dropped in `impl Drop for Run`.
        self.stream.io = std::ptr::null_mut();
    }
}

impl WebSocketStreamMut {
    #[inline]
    fn new(io: &mut WebSocketStream<IoStream>) -> Self {
        Self { io }
    }

    #[inline(always)]
    fn as_pin_mut(&mut self) -> Pin<&mut WebSocketStream<IoStream>> {
        // Safety:
        //
        // The raw pointer at `self.io` is always valid because:
        //
        // - `Run` never moves or reassigns the value stored at `stream`
        // - `Self` only occurs as a field of `Facade`, `Facade` can only occur
        //   as a field of `Run` and `Run` can only occur as `Pin<Box<Run>>`
        Pin::new(unsafe { &mut *self.io })
    }
}

const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<WebSocketStream<IoStream>>();
};

// Safety:
//
// In order for `Run` to act as a supervisor of `Facade`, `Run` must construct
// `Facade` with a mutable reference to it's `stream` field. This makes the
// `facade` field of `Run` self-referential.
//
// To properly facilitate this behavior, `Run` constructs the `facade` field
// with a `*mut WebSocketStream` in `WebSocketStreamMut`. We know that this
// borrow is always valid because:
//
// - `Facade` can only exist as a field of `Run`
//
// - `Run` can only be constructed with a stable heap address as `Pin<Box<Run>>`
//
// - `Run` never moves or reassigns the value stored in the `stream` field
//
// - `Facade` explicitly nulls the `stream` field in it's drop impl and `Run`
//   explicitly drops `facade` before `stream`
//
// There is no reason to provide `WebSocketStreamMut` with a phantom lifetime to
// represent the validity of the `io` becuase it's lifetime is that of self and
// the lifetime of self is the lifetime `Run`.
unsafe impl Send for WebSocketStreamMut {}

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

impl Future for Facade {
    type Output = super::Result;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        #[cfg(debug_assertions)]
        let mut i = 0;

        loop {
            match &mut self.state {
                IoState::Receive => {
                    log!(info(ws = i), "state = receive");
                    indent!(i);

                    // Confirm that the listener can receive the next message.
                    if self.rendezvous.has_capacity()? {
                        // Attempt to pull the next message out of the stream.
                        match self.stream.as_pin_mut().poll_next(cx) {
                            Poll::Ready(Some(Ok(next))) => {
                                // If send fails, the channel is disconnected.
                                self.rendezvous.try_send(next)?;
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
                    if self.listener.as_mut().poll(cx)?.is_ready() {
                        // The listener future is ready and did not error.
                        return Poll::Ready(Ok(()));
                    }

                    if let Some(sent) = self.rendezvous.try_recv()? {
                        self.state = IoState::Send(sent);
                        log!(info(ws = i), "outbound message received from listener.");
                        indent!(i);
                    } else {
                        log!(info(ws = i), "waiting for something interesting to happen.");
                        return Poll::Pending;
                    }
                }

                state @ IoState::Send(_) => {
                    let IoState::Send(item) = mem::replace(state, IoState::Flush) else {
                        // We are in an invalid state. End the session.
                        return Poll::Ready(Ok(()));
                    };

                    log!(info(ws = i), "state = send");
                    indent!(i);

                    match self.stream.as_pin_mut().poll_ready(cx) {
                        Poll::Ready(Ok(_)) => {
                            self.stream.as_pin_mut().start_send(item).map_err(rescue)?;
                            log!(info(ws = i), "outbound message accepted by i/o.");
                            indent!(i);
                        }
                        Poll::Pending => {
                            log!(info(ws = i), "waiting for i/o to become available.");
                            self.state = IoState::Send(item);
                            return Poll::Pending;
                        }
                        Poll::Ready(Err(error)) => {
                            return Poll::Ready(Err(rescue(error)));
                        }
                    }
                }

                IoState::Flush => {
                    log!(info(ws = i), "state = flush");
                    indent!(i);

                    match self.stream.as_pin_mut().poll_flush(cx) {
                        Poll::Pending => {
                            log!(info(ws = i), "waiting for flush to complete.");
                        }
                        Poll::Ready(Ok(_)) => {
                            log!(info(ws = i), "outbound message sent successfully.");
                            self.state = IoState::Receive;
                            cx.waker().wake_by_ref();
                        }
                        Poll::Ready(Err(error)) => {
                            return Poll::Ready(Err(rescue(error)));
                        }
                    }

                    return Poll::Pending;
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
    #[inline(always)]
    fn reconnect(&mut self) -> &mut Facade {
        let (ours, theirs) = Channel::new();
        let request = self.request.clone();
        let facade = Facade {
            listener: Box::pin((self.listener.handle)(theirs, request)),
            state: IoState::Receive,
            stream: WebSocketStreamMut::new(&mut self.stream),
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
                log!(error(ws = 0), "{}", &error);
                Poll::Ready(Err(error))
            }
            Poll::Ready(Err(ControlFlow::Continue(error))) => {
                #[cfg(not(debug_assertions))]
                let _ = error;

                log!(warn(ws = 1), "{}", &error);

                this.reconnect();
                context.waker().wake_by_ref();

                Poll::Pending
            }
        }
    }
}
