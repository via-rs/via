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

use super::error::{already_closed, rescue};
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
            listener: Box::pin((run.listener)(theirs, run.request.clone())),
            state: IoState::Receive,
            stream: &mut run.stream as *mut _,
            rendezvous: ours,
        }
    }

    fn try_recv(&mut self) -> super::Result<Option<Message>> {
        match self.rendezvous.rx().try_recv() {
            Ok(outbound) => Ok(Some(outbound)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Closed) => Err(already_closed()),
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
                    if cfg!(debug_assertions) {
                        println!("info(via::ws): state = Receive");
                    }

                    let Poll::Ready(result) = self.rendezvous.tx().poll_ready(cx) else {
                        if self.listener.as_mut().poll(cx)?.is_ready() {
                            return Poll::Ready(Ok(()));
                        }

                        if let Some(outbound) = self.try_recv()? {
                            self.state = IoState::Send(outbound);
                        };

                        return Poll::Pending;
                    };

                    result.map_err(|_| already_closed())?;

                    if let Poll::Ready(next) = Pin::new(stream).poll_next(cx) {
                        let Some(result) = next else {
                            return Poll::Ready(Ok(()));
                        };

                        let inbound = result.map_err(rescue)?;

                        let Ok(_) = self.rendezvous.tx().try_send(inbound) else {
                            return Poll::Ready(Err(already_closed()));
                        };
                    }

                    if self.listener.as_mut().poll(cx)?.is_ready() {
                        return Poll::Ready(Ok(()));
                    }

                    let Some(outbound) = self.try_recv()? else {
                        return Poll::Pending;
                    };

                    self.state = IoState::Send(outbound);
                }

                state @ IoState::Send(_) => {
                    if cfg!(debug_assertions) {
                        println!("info(via::ws): state = Send");
                    }

                    let mut sink = Pin::new(stream);

                    if sink.as_mut().poll_ready(cx).map_err(rescue)?.is_pending() {
                        return Poll::Pending;
                    }

                    if let IoState::Send(outbound) = mem::replace(state, IoState::Flush) {
                        sink.as_mut().start_send(outbound).map_err(rescue)?;
                    } else {
                        return Poll::Ready(Ok(()));
                    }
                }

                IoState::Flush => {
                    if cfg!(debug_assertions) {
                        println!("info(via::ws): state = Flush");
                    }

                    if Pin::new(stream).poll_flush(cx).map_err(rescue)?.is_ready() {
                        self.state = IoState::Receive;
                    } else {
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
