use futures_channel::mpsc::TryRecvError;
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
        // Determines if we should continue after flush in order to guarantee
        // listener progress.
        //
        // If we are not waiting for I/O, then the listener must be waiting for
        // some future to complete.
        //
        // When true, we can safely assume that the listener future scheduled a
        // wake and yield to the runtime.
        let mut did_poll_listener = false;

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

                    match self.rendezvous.tx().poll_ready(cx) {
                        Poll::Ready(result) => result.map_err(|_| already_closed())?,
                        Poll::Pending => {
                            if cfg!(debug_assertions) {
                                println!("  info(via::ws): tx is waiting for listener progress.");
                            }

                            if self.listener.as_mut().poll(cx)?.is_ready() {
                                return Poll::Ready(Ok(()));
                            }

                            return Poll::Pending;
                        }
                    }

                    if let Poll::Ready(next) = Pin::new(stream).poll_next(cx).map_err(rescue)? {
                        let Some(inbound) = next else {
                            return Poll::Ready(Ok(()));
                        };

                        self.rendezvous
                            .tx()
                            .try_send(inbound)
                            .map_err(|_| already_closed())?;

                        if self.listener.as_mut().poll(cx)?.is_ready() {
                            return Poll::Ready(Ok(()));
                        }

                        did_poll_listener = true;

                        match self.rendezvous.rx().try_recv() {
                            Ok(outbound) => {
                                self.state = IoState::Send(outbound);
                                continue;
                            }
                            Err(TryRecvError::Empty) => {}
                            Err(TryRecvError::Closed) => {
                                return Poll::Ready(Err(already_closed()));
                            }
                        }
                    }

                    return Poll::Pending;
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

                        if !did_poll_listener {
                            continue;
                        }

                        cx.waker().wake_by_ref();
                    }

                    if cfg!(debug_assertions) {
                        println!("  info(via::ws): yielding to runtime. listener already polled.");
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
        let facade = Facade::new(self);
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
                if cfg!(debug_assertions) {
                    eprintln!("error(via::ws): {}", &error);
                }

                Poll::Ready(Err(error))
            }
            Poll::Ready(Err(ControlFlow::Continue(error))) => {
                if cfg!(debug_assertions) {
                    eprintln!("warn(via::ws): {}", &error);
                }

                this.reconnect();
                context.waker().wake_by_ref();

                Poll::Pending
            }
        }
    }
}
