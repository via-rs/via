use base64::engine::{Engine, general_purpose::STANDARD as base64};
use futures_util::{SinkExt, StreamExt};
use http::{Method, StatusCode, header};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use sha1::{Digest, Sha1};
use std::mem;
use std::ops::ControlFlow::{Break, Continue};
use std::sync::Arc;

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::WebSocketStream;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use super::error::{WebSocketError, into_control_flow};
use super::{Channel, Request};
use crate::{BoxFuture, Error, Middleware, Next, Response, raise};

const DEFAULT_FRAME_SIZE: usize = 16384; // 16KB
const WS_ACCEPT_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub struct Ws<T> {
    listener: Arc<T>,
    config: WsConfig,
}

#[derive(Clone, Debug)]
struct WsConfig {
    buffer_size: usize,
    max_frame_size: Option<usize>,
    max_message_size: Option<usize>,
}

macro_rules! debug {
    (#[error] $error:expr) => {
        debug!("error(ws): {}", $error)
    };
    ($($args:tt)*) => {{
        if cfg!(debug_assertions) {
            eprintln!($($args)*);
        }
    }};
}

fn gen_accept_key(key: &[u8]) -> String {
    let mut hasher = Sha1::new();

    hasher.update(key);
    hasher.update(WS_ACCEPT_GUID);

    base64.encode(hasher.finalize())
}

#[cfg(feature = "tokio-tungstenite")]
async fn handshake(
    ws_config: WsConfig,
    request: &mut http::Request<()>,
) -> Result<WebSocketStream<TokioIo<Upgraded>>, Error> {
    use tungstenite::protocol::{Role, WebSocketConfig};

    let upgraded = TokioIo::new(hyper::upgrade::on(request).await?);
    let mut config = WebSocketConfig::default()
        .accept_unmasked_frames(false)
        .read_buffer_size(ws_config.buffer_size)
        .max_frame_size(ws_config.max_frame_size)
        .max_message_size(ws_config.max_message_size);

    if let Some(capacity) = ws_config
        .max_message_size
        .and_then(|limit| limit.checked_mul(2))
    {
        config = config.write_buffer_size(capacity);
    }

    Ok(WebSocketStream::from_raw_socket(upgraded, Role::Server, Some(config)).await)
}

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
async fn handshake(
    ws_config: WsConfig,
    request: &mut http::Request<()>,
) -> Result<WebSocketStream<TokioIo<Upgraded>>, Error> {
    use tokio_websockets::server::Builder;
    use tokio_websockets::{Config, Limits};

    let stream = TokioIo::new(hyper::upgrade::on(request).await?);
    let limits = Limits::default().max_payload_len(ws_config.max_message_size);
    let config = Config::default()
        .frame_size(ws_config.max_frame_size.unwrap_or(DEFAULT_FRAME_SIZE))
        .flush_threshold(DEFAULT_FRAME_SIZE);

    Ok(Builder::new().config(config).limits(limits).serve(stream))
}

async fn run<T, App, Await>(
    mut stream: WebSocketStream<TokioIo<Upgraded>>,
    listener: Arc<T>,
    request: Request<App>,
) where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Await: Future<Output = super::Result> + Send,
{
    loop {
        let (facade, mut rendezvous) = Channel::new();
        let mut listen = Box::pin(listener(facade, request.clone()));
        let trx = async {
            loop {
                tokio::select! {
                    // Receive a message from the channel and send it to the stream.
                    Some(message) = rendezvous.recv() => {
                        stream.send(message).await.map_err(into_control_flow)?;
                    }
                    // Receive a message from the stream and send it to the channel.
                    next = stream.next() => {
                        let message = match next {
                            Some(result) => result.map_err(into_control_flow)?,
                            None => break Ok(()),
                        };

                        if rendezvous.send(message).await.is_err() {
                            let error = WebSocketError::AlreadyClosed.into();
                            break Err(Break(Some(error)));
                        }
                    }
                }
            }
        };

        break tokio::select! {
            ref result = &mut listen => {
                if let Err(op @ (Break(error) | Continue(error))) = result {
                    debug!(#[error] error);
                    if op.is_continue() {
                        continue;
                    }
                }
            }
            ref result = trx => match result {
                Ok(_) => {
                    if let Err(Break(error) | Continue(error)) = &listen.await {
                        debug!(#[error] error);
                    }
                }
                Err(Break(err)) => {
                    if let Some(error) = err {
                        debug!(#[error] error);
                    }
                }
                Err(Continue(error)) => {
                    debug!(#[error] error);
                    continue;
                }
            },
        };
    }

    debug!("info(ws): websocket session ended");
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
            let mut request = Request::new(request);
            let listener = Arc::clone(&self.listener);
            let config = self.config.clone();

            async move {
                let mut upgradeable = http::Request::new(());
                let Some(envelope) = Arc::get_mut(request.envelope_mut()) else {
                    eprintln!("an error occurred while upgrading the connection to a websocket");
                    return;
                };

                mem::swap(upgradeable.extensions_mut(), envelope.extensions_mut());

                match handshake(config, &mut upgradeable).await {
                    Ok(stream) => {
                        mem::swap(upgradeable.extensions_mut(), envelope.extensions_mut());
                        run(stream, listener, request).await;
                    }
                    Err(error) => {
                        eprintln!("error(upgrade): {}", error);
                    }
                }
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

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_FRAME_SIZE,
            max_frame_size: Some(DEFAULT_FRAME_SIZE),
            max_message_size: Some(DEFAULT_FRAME_SIZE),
        }
    }
}
