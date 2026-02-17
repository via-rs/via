use base64::engine::{Engine, general_purpose::STANDARD as base64};
use futures_util::{SinkExt, StreamExt};
use http::{Method, StatusCode, header};
use hyper::upgrade::{OnUpgrade, Upgraded};
use hyper_util::rt::TokioIo;
use sha1::{Digest, Sha1};
use std::ops::ControlFlow::{Break, Continue};
use std::sync::Arc;

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::WebSocketStream;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use super::error::try_rescue;
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

fn gen_accept_key(key: &[u8]) -> String {
    let mut hasher = Sha1::new();

    hasher.update(key);
    hasher.update(WS_ACCEPT_GUID);

    base64.encode(hasher.finalize())
}

#[cfg(feature = "tokio-tungstenite")]
async fn handshake(
    on_upgrade: OnUpgrade,
    config: WsConfig,
) -> Result<WebSocketStream<TokioIo<Upgraded>>, Error> {
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
        TokioIo::new(on_upgrade.await?),
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
) -> Result<WebSocketStream<TokioIo<Upgraded>>, Error> {
    use tokio_websockets::server::Builder;
    use tokio_websockets::{Config, Limits};

    let limits = Limits::default().max_payload_len(config.max_message_size);
    let config = Config::default()
        .frame_size(config.max_frame_size.unwrap_or(DEFAULT_FRAME_SIZE))
        .flush_threshold(DEFAULT_FRAME_SIZE);

    Ok(Builder::new()
        .config(config)
        .limits(limits)
        .serve(TokioIo::new(on_upgrade.await?)))
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
                    next = rendezvous.recv() => match next {
                        Some(message) => stream.send(message).await.map_err(try_rescue)?,
                        None => break Ok(()),
                    },
                    // Receive a message from the stream and send it to the channel.
                    next = stream.next() => match next {
                        Some(result) => rendezvous.send(result.map_err(try_rescue)?).await?,
                        None => break Ok(()),
                    },
                }
            }
        };

        let err = tokio::select! {
            result = &mut listen => result.err(),
            result = trx => match result {
                Ok(_) => listen.await.err(),
                Err(error) => Some(error),
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

        let Some(accept) = request
            .headers()
            .get(header::SEC_WEBSOCKET_KEY)
            .map(|value| gen_accept_key(value.as_bytes()))
        else {
            return Box::pin(async {
                raise!(400, message = "missing required header: sec-websocket-key");
            });
        };

        tokio::spawn({
            let Some(upgrade) = request.extensions_mut().remove::<OnUpgrade>() else {
                return Box::pin(async {
                    raise!(message = "connection does not support websocket upgrades");
                });
            };

            let listener = Arc::clone(&self.listener);
            let request = Request::new(request);
            let config = self.config.clone();

            async move {
                match handshake(upgrade, config).await {
                    Ok(stream) => {
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
