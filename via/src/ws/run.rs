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

use crate::Error;

use super::error::try_rescue;
use super::io::UpgradedIo;
use super::{Channel, Message, Request};

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
            state: IoState::Receive,
            stream: &mut run.stream as *mut _,
            listener: Box::pin((run.listener)(theirs, run.request.clone())),
            rendezvous: ours,
        }
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
                        }
                        Err(TryRecvError::Empty) => {
                            self.state = IoState::Receive;
                            return Poll::Pending;
                        }
                        Err(TryRecvError::Closed) => {
                            return Poll::Ready(Ok(()));
                        }
                    }
                }

                state @ IoState::Send(_) => {
                    let IoState::Send(message) = mem::replace(state, IoState::Flush) else {
                        *state = IoState::Receive;
                        return Poll::Pending;
                    };

                    let mut sink = Pin::new(stream);

                    let Poll::Ready(result) = sink.as_mut().poll_ready(context) else {
                        *state = IoState::Send(message);
                        return Poll::Pending;
                    };

                    if let Err(error) = result.and_then(|_| sink.start_send(message)) {
                        *state = IoState::Receive;
                        return Poll::Ready(Err(try_rescue(error)));
                    }
                }

                IoState::Receive => {
                    if self.rendezvous.tx().poll_ready(context).is_pending() {
                        return Poll::Pending;
                    }

                    let option = ready!(Pin::new(stream).poll_next(context));
                    self.state = IoState::Listen;

                    if let Some(result) = option {
                        let message = result.map_err(try_rescue)?;

                        if self.rendezvous.tx().try_send(message).is_err() {
                            return Poll::Ready(Ok(()));
                        }
                    }
                }

                IoState::Flush => {
                    let result = ready!(Pin::new(stream).poll_flush(context));
                    self.state = IoState::Receive;

                    result.map_err(try_rescue)?;
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
