use futures_channel::mpsc::TryRecvError;
use futures_core::Stream;
use futures_sink::Sink;
use std::future::Future;
use std::marker::PhantomPinned;
use std::mem;
use std::ops::ControlFlow::{Break, Continue};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::WebSocketStream;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use super::error::{WebSocketError, try_rescue};
use super::io::UpgradedIo;
use super::{Channel, Message, Request};
use crate::Error;

pub struct RunTask<T, App> {
    run: Pin<Box<Run<T, App>>>,
}

enum IoState {
    Listen,
    Send(Message),
    Flush,
    Receive,
}

struct Facade {
    /// A flag used to determine whether or not the listener was polled.
    ///
    /// The listener should be polled regardless of the state that we are in
    /// when we wake up. The only circumstance in which the listener is not
    /// polled is when we are waiting on I/O.
    ///
    /// This behavior mirrors what happens when you work directly with a
    /// `WebSocketStream` while obscuring the actual I/O behind our
    /// rendezvous channel.
    did_poll_listener: bool,

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
    Await: Future<Output = super::Result> + Send,
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

impl Facade {
    fn new<T, App, Await>(run: &mut Run<T, App>) -> Self
    where
        T: Fn(Channel, Request<App>) -> Await + Send,
        Await: Future<Output = super::Result> + Send + 'static,
    {
        let (ours, theirs) = Channel::new();

        Self {
            did_poll_listener: false,
            listener: Box::pin((run.listener)(theirs, run.request.clone())),
            state: IoState::Receive,
            stream: &mut run.stream as *mut _,
            rendezvous: ours,
        }
    }

    fn error(&mut self, error: WebSocketError) -> Poll<super::Result> {
        self.state = IoState::Receive;
        self.did_poll_listener = false;
        return Poll::Ready(Err(try_rescue(error)));
    }

    fn park(&mut self) -> Poll<super::Result> {
        self.did_poll_listener = false;
        return Poll::Pending;
    }
}

unsafe impl Send for Facade {}

impl Drop for Facade {
    fn drop(&mut self) {
        // Set the bit at `did_poll_listener` to zero.
        self.did_poll_listener = false;

        // Null out the raw pointer to prevent accidental use.
        // The rest of the fields in self are dropped automatically.
        self.stream = std::ptr::null_mut();
    }
}

impl Future for Facade {
    type Output = super::Result;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        loop {
            // Safety:
            //
            // stream is a mutable pointer to the stream field of the `Run<_, _>`
            // struct. Run is structurally pinned in a `Pin<Box<_>>` as a field
            // of `RunTask`. The only way to construct `Run<_, _>` is with the
            // `RunTask::new` fn.
            let stream = unsafe { &mut *self.stream };

            match &mut self.state {
                IoState::Listen => {
                    if self.listener.as_mut().poll(context)?.is_ready() {
                        return Poll::Ready(Ok(()));
                    }

                    match self.rendezvous.rx().try_recv() {
                        Ok(message) => {
                            self.state = IoState::Send(message);
                            self.did_poll_listener = true;
                        }
                        Err(TryRecvError::Empty) => {
                            self.state = IoState::Receive;
                            self.did_poll_listener = false;
                        }
                        Err(TryRecvError::Closed) => {
                            return Poll::Ready(Ok(()));
                        }
                    }
                }

                state @ IoState::Send(_) => {
                    let mut sink = Pin::new(stream);

                    match sink.as_mut().poll_ready(context) {
                        Poll::Pending => return self.park(),
                        Poll::Ready(Ok(_)) => {}
                        Poll::Ready(Err(error)) => return self.error(error),
                    }

                    let IoState::Send(message) = mem::replace(state, IoState::Flush) else {
                        *state = IoState::Receive;
                        return self.park();
                    };

                    if let Err(error) = sink.start_send(message) {
                        return self.error(error);
                    }
                }

                IoState::Receive => {
                    match self.rendezvous.tx().poll_ready(context) {
                        Poll::Pending => return self.park(),
                        Poll::Ready(Ok(_)) => {}
                        Poll::Ready(Err(_)) => return Poll::Ready(Ok(())),
                    }

                    let message = match Pin::new(stream).poll_next(context) {
                        Poll::Pending => return self.park(),
                        Poll::Ready(Some(Ok(next))) => next,
                        Poll::Ready(Some(Err(error))) => return self.error(error),
                        Poll::Ready(None) => return Poll::Ready(Ok(())),
                    };

                    let _ = self.rendezvous.tx().try_send(message);
                    self.state = IoState::Listen;

                    if self.did_poll_listener {
                        return self.park();
                    }
                }

                IoState::Flush => {
                    match Pin::new(stream).poll_flush(context) {
                        Poll::Ready(Err(error)) => return self.error(error),
                        Poll::Ready(Ok(_)) => self.state = IoState::Receive,
                        Poll::Pending => return self.park(),
                    };
                }
            }
        }
    }
}

impl<T, App, Await> Run<T, App>
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Await: Future<Output = super::Result> + Send,
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
            None => {
                let facade = Facade::new(this);
                this.facade = Some(facade);

                // Safety: We just assigned a Some value to this.facade.
                unsafe { this.facade.as_mut().unwrap_unchecked() }
            }
        };

        match ready!(Pin::new(future).poll(context)) {
            Ok(_) => Poll::Ready(Ok(())),
            Err(Break(error)) => {
                if cfg!(debug_assertions) {
                    eprintln!("error(ws): {}", &error);
                }

                Poll::Ready(Err(error))
            }
            Err(Continue(error)) => {
                if cfg!(debug_assertions) {
                    eprintln!("warn(ws): {}", &error);
                }

                this.facade = None;
                Poll::Pending
            }
        }
    }
}
