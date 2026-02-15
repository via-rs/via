use base64::engine::{Engine, general_purpose::STANDARD as base64};
use futures_util::{SinkExt, StreamExt};
use http::{Method, StatusCode, header};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use std::mem::swap;
use std::ops::ControlFlow;
use std::sync::Arc;
use tokio::task::coop;
use tungstenite::protocol::{Role, WebSocketConfig};

use super::channel::Channel;
use super::error::{WebSocketError, is_recoverable};
use crate::request::Envelope;
use crate::{BoxFuture, Error, Middleware, Next, Response, Shared, raise};

const DEFAULT_FRAME_SIZE: usize = 16384; // 16KB
const WS_ACCEPT_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

type WebSocketStream = tokio_tungstenite::WebSocketStream<TokioIo<Upgraded>>;

#[derive(Debug)]
pub struct Request<App> {
    envelope: Arc<Envelope>,
    app: Shared<App>,
}

pub struct Ws<T> {
    listener: Arc<T>,
    config: WebSocketConfig,
}

macro_rules! match_control_flow {
    ($break:tt on $flow:expr) => {{
        use std::ops::ControlFlow;

        match $flow {
            ControlFlow::Break(err) => {
                if let Some(error) = &err
                    && cfg!(debug_assertions)
                {
                    eprintln!("error(ws): {}", error);
                }

                $break;
            }
            ControlFlow::Continue(error) => {
                if cfg!(debug_assertions) {
                    eprintln!("warn(ws): {}", error);
                }
            }
        }
    }};
}

fn from_ws_error(error: WebSocketError) -> ControlFlow<Option<Error>, Error> {
    if is_recoverable(&error) {
        ControlFlow::Continue(error.into())
    } else {
        ControlFlow::Break(Some(error.into()))
    }
}

fn gen_accept_key(key: &[u8]) -> String {
    #[cfg(feature = "aws-lc-rs")]
    let mut hasher = aws_lc_rs::digest::Context::new(&aws_lc_rs::digest::SHA1_FOR_LEGACY_USE_ONLY);

    #[cfg(feature = "ring")]
    let mut hasher = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);

    hasher.update(key);
    hasher.update(WS_ACCEPT_GUID);
    base64.encode(hasher.finish())
}

#[inline]
async fn handshake(
    config: WebSocketConfig,
    request: &mut http::Request<()>,
) -> Result<WebSocketStream, Error> {
    let upgraded = TokioIo::new(hyper::upgrade::on(request).await?);
    Ok(WebSocketStream::from_raw_socket(upgraded, Role::Server, Some(config)).await)
}

async fn run<T, App, Await>(mut stream: WebSocketStream, listener: Arc<T>, request: Request<App>)
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Await: Future<Output = super::Result> + Send,
{
    loop {
        let (rendezvous, transport) = Channel::new();
        let listen = Box::pin(listener(rendezvous, request.clone()));
        let trx = async {
            let (tx, mut rx) = transport;

            loop {
                tokio::select! {
                    // Receive a message from the channel and send it to the stream.
                    Some(message) = coop::unconstrained(rx.recv()) => {
                        stream.send(message).await.map_err(from_ws_error)?;
                    }
                    // Receive a message from the stream and send it to the channel.
                    next = stream.next() => {
                        let message = match next {
                            Some(result) => result.map_err(from_ws_error)?,
                            None => break Ok(()),
                        };

                        if tx.send(message).await.is_err() {
                            let error = WebSocketError::AlreadyClosed.into();
                            break Err(ControlFlow::Break(Some(error)));
                        }
                    }
                }
            }
        };

        match_control_flow!(break on tokio::select! {
            // Send and receive messages to and from the channel.
            result = trx => {
                result.map_or_else(|error| error, |_| ControlFlow::Break(None))
            }
            // The future returned from the listener is ready.
            result = listen => {
                result.map_or_else(|error| error.map_break(Some), |_| ControlFlow::Break(None))
            }
        });
    }

    if cfg!(debug_assertions) {
        eprintln!("info(ws): websocket session ended");
    }
}

impl<App> Request<App> {
    fn upgraded(request: crate::Request<App>) -> Self {
        let (envelope, _, app) = request.into_parts();

        Self {
            envelope: Arc::new(envelope),
            app,
        }
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    pub fn envelope(&self) -> &Envelope {
        &self.envelope
    }

    pub fn to_owned_app(&self) -> Shared<App> {
        self.app.clone()
    }
}

impl<App> Clone for Request<App> {
    fn clone(&self) -> Self {
        Self {
            envelope: Arc::clone(&self.envelope),
            app: self.app.clone(),
        }
    }
}

impl<T> Ws<T> {
    pub(super) fn new(upgraded: T) -> Self {
        Self {
            listener: Arc::new(upgraded),
            config: WebSocketConfig::default()
                .accept_unmasked_frames(false)
                .read_buffer_size(DEFAULT_FRAME_SIZE)
                .write_buffer_size(DEFAULT_FRAME_SIZE)
                .max_frame_size(Some(DEFAULT_FRAME_SIZE))
                .max_message_size(Some(DEFAULT_FRAME_SIZE)),
        }
    }

    /// The frame size used for messages in bytes.
    ///
    /// **Default:** `16 KB`
    ///
    pub fn frame_size(self, frame_size: usize) -> Self {
        Self {
            config: self.config.max_frame_size(Some(frame_size)),
            ..self
        }
    }

    /// The maximum message size in bytes.
    ///
    /// **Default:** `16 KB`
    ///
    pub fn max_message_size(self, max_message_size: Option<usize>) -> Self {
        Self {
            config: self.config.max_message_size(max_message_size),
            ..self
        }
    }

    /// Read buffer capacity. The read buffer is allocated eagerly and is used
    /// for receiving messages.
    ///
    /// **Default:** `16 KB`
    ///
    pub fn read_buffer_size(self, buffer_size: usize) -> Self {
        Self {
            config: self.config.read_buffer_size(buffer_size),
            ..self
        }
    }
}

impl<T, App, Await> Middleware<App> for Ws<T>
where
    T: Fn(Channel, Request<App>) -> Await + Send + Sync + 'static,
    App: Send + Sync + 'static,
    Await: Future<Output = super::Result> + Send,
{
    fn call(&self, request: crate::Request<App>, next: Next<App>) -> BoxFuture {
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

        let Some(accept) = request
            .headers()
            .get(header::SEC_WEBSOCKET_KEY)
            .map(|value| gen_accept_key(value.as_bytes()))
        else {
            return Box::pin(async {
                raise!(400, message = "missing required header: sec-websocket-key.")
            });
        };

        tokio::spawn({
            let config = self.config;
            let listener = Arc::clone(&self.listener);
            let mut request = request;

            async move {
                let mut upgradeable = http::Request::new(());

                swap(request.extensions_mut(), upgradeable.extensions_mut());
                match handshake(config, &mut upgradeable).await {
                    Ok(stream) => {
                        swap(request.extensions_mut(), upgradeable.extensions_mut());
                        run(stream, listener, Request::upgraded(request)).await
                    }
                    Err(error) => {
                        eprintln!("error(upgrade): {}", error);
                    }
                };
            }
        });

        Box::pin(async {
            Response::build()
                .status(StatusCode::SWITCHING_PROTOCOLS)
                .header(header::CONNECTION, "upgrade")
                .header(header::SEC_WEBSOCKET_ACCEPT, accept)
                .header(header::UPGRADE, "websocket")
                .finish()
        })
    }
}
