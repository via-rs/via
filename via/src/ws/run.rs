use futures_core::{FusedFuture, Future, Stream};
use futures_sink::Sink;
use std::marker::PhantomPinned;
use std::ops::ControlFlow::{Break, Continue};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::WebSocketStream;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use crate::Error;

use super::error::{WebSocketError, try_rescue};
use super::io::UpgradedIo;
use super::{Channel, Message, Request};

pub struct RunTask<T, App> {
    run: Pin<Box<Run<T, App>>>,
}

enum IoState {
    Receive,
    Listen,
    Send,
    Flush,
}

struct Facade {
    state: IoState,
    stream: *mut WebSocketStream<UpgradedIo>,
    listener: Pin<Box<dyn Future<Output = super::Result> + Send>>,
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

unsafe impl Send for Facade {}

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

impl Future for Facade {
    type Output = super::Result;

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        todo!()
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
            Some(facade) => Pin::new(facade),
            None => {
                let facade = Facade::new(this);
                this.facade = Some(facade);
                // Safety: We just assigned a Some value to this.facade.
                Pin::new(unsafe { this.facade.as_mut().unwrap_unchecked() })
            }
        };

        match ready!(future.poll(context)) {
            Ok(_) => Poll::Ready(Ok(())),
            Err(Break(error)) => Poll::Ready(Err(error)),
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
