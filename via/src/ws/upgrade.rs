use futures_core::{FusedFuture, Future, Stream};
use futures_sink::Sink;
use http::{Method, StatusCode, header};
use hyper::upgrade::OnUpgrade;
use std::ops::ControlFlow::{Break, Continue};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::WebSocketStream;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use super::error::{WebSocketError, try_rescue};
use super::io::UpgradedIo;
use super::sha1::sha1;
use super::{Channel, Message, Request};
use crate::{BoxFuture, Error, Middleware, Next, Response, raise};

const DEFAULT_FRAME_SIZE: usize = 16384; // 16KB

pub struct Ws<T> {
    listener: Arc<T>,
    config: WsConfig,
}

enum Dispatch {
    Done,
    Out(Option<Message>),
    In(Result<Message, WebSocketError>),
}

enum ForwardState {
    Done,
    Flush,
    Waiting(Message),
}

struct Forward<'a> {
    stream: Pin<&'a mut WebSocketStream<UpgradedIo>>,
    state: ForwardState,
}

struct Receive<'a, T> {
    stream: Pin<&'a mut WebSocketStream<UpgradedIo>>,
    recv: T,
}

struct ReceiveProjection<'a, T> {
    stream: Pin<&'a mut WebSocketStream<UpgradedIo>>,
    recv: Pin<&'a mut T>,
}

#[derive(Clone, Debug)]
struct WsConfig {
    buffer_size: usize,
    max_frame_size: Option<usize>,
    max_message_size: Option<usize>,
}

#[cfg(feature = "tokio-tungstenite")]
async fn handshake(
    on_upgrade: OnUpgrade,
    config: WsConfig,
) -> Result<WebSocketStream<UpgradedIo>, Error> {
    use tungstenite::protocol::{Role, WebSocketConfig};

    let max_message_size = config.max_message_size;
    let mut config = WebSocketConfig::default()
        .accept_unmasked_frames(false)
        .read_buffer_size(config.buffer_size)
        .max_frame_size(config.max_frame_size)
        .max_message_size(max_message_size);

    if let Some(capacity) = max_message_size.and_then(|limit| limit.checked_mul(2)) {
        config = config.write_buffer_size(capacity);
    }

    let stream = WebSocketStream::from_raw_socket(
        UpgradedIo::new(on_upgrade.await?),
        Role::Server,
        Some(config),
    )
    .await;

    Ok(stream)
}

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
async fn handshake(
    on_upgrade: OnUpgrade,
    config: WsConfig,
) -> Result<WebSocketStream<UpgradedIo>, Error> {
    use tokio_websockets::server::Builder;
    use tokio_websockets::{Config, Limits};

    let limits = Limits::default().max_payload_len(config.max_message_size);
    let config = Config::default()
        .frame_size(config.max_frame_size.unwrap_or(DEFAULT_FRAME_SIZE))
        .flush_threshold(DEFAULT_FRAME_SIZE);

    Ok(Builder::new()
        .config(config)
        .limits(limits)
        .serve(UpgradedIo::new(on_upgrade.await?)))
}

async fn run<T, App, Await>(
    stream: WebSocketStream<UpgradedIo>,
    listener: Arc<T>,
    request: Request<App>,
) where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Await: Future<Output = super::Result> + Send,
{
    tokio::pin!(stream);

    loop {
        let (facade, mut rendezvous) = Channel::new();
        let mut listen = Box::pin(listener(facade, request.clone()));
        let trx = async {
            loop {
                return match Receive::new(stream.as_mut(), rendezvous.recv()).await {
                    Dispatch::Done => Ok(true),

                    Dispatch::Out(None) => Ok(false),
                    Dispatch::Out(Some(message)) => {
                        let forward = Forward::new(stream.as_mut(), message);
                        if let Err(error) = forward.await {
                            Err(try_rescue(error))
                        } else {
                            continue;
                        }
                    }

                    Dispatch::In(result) => {
                        let message = result.map_err(try_rescue)?;
                        rendezvous.send(message).await?;
                        continue;
                    }
                };
            }
        };

        let err = tokio::select! {
            result = listen.as_mut() => result.err(),
            result = trx => match result {
                Ok(graceful) if graceful => listen.await.err(),
                Err(error) => Some(error),
                _ => None,
            },
        };

        if let Some(op @ (Break(error) | Continue(error))) = err.as_ref() {
            if cfg!(debug_assertions) {
                eprintln!("error(ws): {}", error);
            }

            if op.is_continue() {
                continue;
            }
        }

        break;
    }

    if cfg!(debug_assertions) {
        eprintln!("info(ws): websocket session ended");
    }
}

impl<'a> Forward<'a> {
    fn new(stream: Pin<&'a mut WebSocketStream<UpgradedIo>>, message: Message) -> Self {
        Self {
            stream,
            state: ForwardState::Waiting(message),
        }
    }
}

impl<'a> Future for Forward<'a> {
    type Output = Result<(), WebSocketError>;

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        let this = self.get_mut();

        loop {
            match &mut this.state {
                ForwardState::Done => {
                    return Poll::Ready(Ok(()));
                }
                state @ ForwardState::Flush => {
                    let flush = this.stream.as_mut().poll_flush(context);

                    if flush.is_ready() {
                        *state = ForwardState::Done;
                    }

                    return flush;
                }
                state @ ForwardState::Waiting(_) => {
                    let Poll::Ready(ready) = this.stream.as_mut().poll_ready(context) else {
                        return Poll::Pending;
                    };

                    if ready.is_err() {
                        *state = ForwardState::Done;
                        return Poll::Ready(ready);
                    }

                    let ForwardState::Waiting(message) =
                        std::mem::replace(state, ForwardState::Flush)
                    else {
                        unreachable!();
                    };

                    if let Err(error) = this.stream.as_mut().start_send(message) {
                        *state = ForwardState::Done;
                        return Poll::Ready(Err(error));
                    }
                }
            };
        }
    }
}

impl<'a> FusedFuture for Forward<'a> {
    fn is_terminated(&self) -> bool {
        matches!(self.state, ForwardState::Done)
    }
}

impl<'a, T> Receive<'a, T> {
    fn new(stream: Pin<&'a mut WebSocketStream<UpgradedIo>>, recv: T) -> Self {
        Self { stream, recv }
    }
}

impl<'a, T> Receive<'a, T> {
    #[inline]
    fn project(self: Pin<&mut Self>) -> ReceiveProjection<'_, T> {
        // Safety: A pin projection. Neither data or refs move out of self.
        unsafe {
            let this = self.get_unchecked_mut();

            ReceiveProjection {
                stream: this.stream.as_mut(),
                recv: Pin::new_unchecked(&mut this.recv),
            }
        }
    }
}

impl<'a, T> Future for Receive<'a, T>
where
    T: Future<Output = Option<Message>>,
{
    type Output = Dispatch;

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        let this = self.project();

        if let Poll::Ready(next) = this.recv.poll(context) {
            Poll::Ready(Dispatch::Out(next))
        } else if let Poll::Ready(next) = this.stream.poll_next(context) {
            Poll::Ready(next.map_or(Dispatch::Done, Dispatch::In))
        } else {
            Poll::Pending
        }
    }
}

impl<T> Ws<T> {
    pub(super) fn new(listener: T) -> Self {
        Self {
            listener: Arc::new(listener),
            config: WsConfig::default(),
        }
    }

    /// The amount of memory to pre-allocate in bytes for buffered reads.
    ///
    /// **Default:** `16 KB`
    ///
    pub fn buffer_size(mut self, capacity: usize) -> Self {
        self.config.buffer_size = capacity;
        self
    }

    /// The maximum size of a single incoming message frame.
    ///
    /// A `None` value indicates no frame size limit.
    ///
    /// **Default:** `16 KB`
    ///
    pub fn max_frame_size(mut self, limit: Option<usize>) -> Self {
        self.config.max_frame_size = limit;
        self
    }

    /// The maximum message size in bytes.
    ///
    /// **Default:** `16 KB`
    ///
    pub fn max_message_size(mut self, limit: Option<usize>) -> Self {
        self.config.max_message_size = limit;
        self
    }
}

impl<T, App, Await> Middleware<App> for Ws<T>
where
    T: Fn(Channel, Request<App>) -> Await + Send + Sync + 'static,
    App: Send + Sync + 'static,
    Await: Future<Output = super::Result> + Send,
{
    fn call(&self, mut request: crate::Request<App>, next: Next<App>) -> BoxFuture {
        // Confirm that the request is for a websocket upgrade.
        if request.method() != Method::GET
            || !request
                .headers()
                .get(header::CONNECTION)
                .zip(request.headers().get(header::UPGRADE))
                .is_some_and(|(connection, upgrade)| {
                    connection.as_bytes().eq_ignore_ascii_case(b"upgrade")
                        && upgrade.as_bytes().eq_ignore_ascii_case(b"websocket")
                })
        {
            return next.call(request);
        }

        if request
            .headers()
            .get(header::SEC_WEBSOCKET_VERSION)
            .is_none_or(|value| value.as_bytes() != b"13")
        {
            return Box::pin(async {
                raise!(400, message = "sec-websocket-version header must be \"13\"");
            });
        }

        let accept = match request
            .headers()
            .get(header::SEC_WEBSOCKET_KEY)
            .map(|value| sha1(value.as_bytes()))
        {
            Some(Ok(buf)) => buf,
            Some(Err(error)) => return Box::pin(async { Err(error) }),
            None => {
                return Box::pin(async {
                    raise!(400, message = "missing required header: sec-websocket-key");
                });
            }
        };

        let Some(upgrade) = request.extensions_mut().remove::<OnUpgrade>() else {
            return Box::pin(async {
                raise!(message = "connection does not support websocket upgrades");
            });
        };

        let listener = Arc::clone(&self.listener);
        let request = Request::new(request);
        let config = self.config.clone();

        Box::pin(async move {
            tokio::spawn(Box::pin(async move {
                match handshake(upgrade, config).await {
                    Ok(stream) => {
                        run(stream, listener, request).await;
                    }
                    Err(error) => {
                        eprintln!("error(upgrade): {}", error);
                    }
                }
            }));

            Response::build()
                .status(StatusCode::SWITCHING_PROTOCOLS)
                .header(header::CONNECTION, "upgrade")
                .header(
                    header::SEC_WEBSOCKET_ACCEPT,
                    str::from_utf8(accept.as_slice())?,
                )
                .header(header::UPGRADE, "websocket")
                .finish()
        })
    }
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_FRAME_SIZE,
            max_frame_size: Some(DEFAULT_FRAME_SIZE),
            max_message_size: Some(DEFAULT_FRAME_SIZE),
        }
    }
}
