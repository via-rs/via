use http::{HeaderValue, StatusCode, header};
use hyper::upgrade::OnUpgrade;
use std::future::Future;
use std::sync::Arc;

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::WebSocketStream;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use super::io::UpgradedIo;
use super::run::RunTask;
use super::util::sha1;
use super::{Channel, Request};
use crate::{BoxFuture, Error, Middleware, Next, Response, err};

const DEFAULT_FRAME_SIZE: usize = 16384; // 16KB

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

#[inline(always)]
fn has_token(header: &HeaderValue, token: &str) -> bool {
    header.to_str().is_ok_and(|input| {
        input
            .split(',')
            .any(|value| value.trim_ascii().eq_ignore_ascii_case(token))
    })
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
    Await: Future<Output = super::Result> + Send + 'static,
{
    fn call(&self, mut request: crate::Request<App>, next: Next<App>) -> BoxFuture {
        // Confirm that the request is for a websocket upgrade.
        if !request
            .headers()
            .get(header::CONNECTION)
            .zip(request.headers().get(header::UPGRADE))
            .is_some_and(|(connection, upgrade)| {
                has_token(connection, "upgrade") && has_token(upgrade, "websocket")
            })
        {
            return next.call(request);
        }

        if request
            .headers()
            .get(header::SEC_WEBSOCKET_VERSION)
            .is_none_or(|value| value.as_bytes() != b"13")
        {
            return Box::pin(async { Err(err!(426, "sec-websocket-version must be \"13\".")) });
        }

        let accept = match request
            .headers()
            .get(header::SEC_WEBSOCKET_KEY)
            .and_then(|value| value.to_str().ok().map(sha1))
            .unwrap_or_else(|| Err(err!(400, "missing required header \"sec-websocket-key\"")))
        {
            Ok(digest) => digest,
            Err(error) => return Box::pin(async { Err(error) }),
        };

        let Some(upgrade) = request.extensions_mut().remove::<OnUpgrade>() else {
            return Box::pin(async {
                Err(err!(500, "connection does not support websocket upgrades"))
            });
        };

        let listener = Arc::clone(&self.listener);
        let config = self.config.clone();

        Box::pin(async move {
            let request = Request::new(request);

            tokio::spawn(async move {
                let stream = handshake(upgrade, config).await?;
                RunTask::new(listener, request, stream).await
            });

            Response::build()
                .status(StatusCode::SWITCHING_PROTOCOLS)
                .header(header::CONNECTION, "upgrade")
                .header(header::SEC_WEBSOCKET_ACCEPT, accept.as_str())
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
