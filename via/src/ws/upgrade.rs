use http::{HeaderMap, StatusCode, header};
use hyper::upgrade::OnUpgrade;
use std::future::Future;
use std::sync::Arc;

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::WebSocketStream;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use super::Channel;
use super::io::UpgradedIo;
use super::run::RunTask;
use super::util::{Base64EncodedDigest, sha1};
use crate::guard::header::{self as h, Contains, Header, Tag, TagNoCase};
use crate::guard::{self, Predicate};
use crate::ws::error::UpgradeError;
use crate::{BoxFuture, Error, Middleware, Next, Request, Response, ResultExt, deny};

const DEFAULT_FRAME_SIZE: usize = 16384; // 16KB

pub struct Ws<T> {
    listener: Arc<Listener<T>>,
    guard: (
        Header<Tag>,
        Header<Contains<TagNoCase>>,
        Header<Contains<TagNoCase>>,
    ),
}

pub(super) struct Listener<T> {
    pub(super) handle: T,
    config: WsConfig,
}

#[derive(Clone, Debug)]
struct WsConfig {
    buffer_size: usize,
    max_frame_size: Option<usize>,
    max_message_size: Option<usize>,
}

#[cfg(feature = "tokio-tungstenite")]
async fn handshake(
    config: &WsConfig,
    upgrade: OnUpgrade,
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
        UpgradedIo::new(upgrade.await?),
        Role::Server,
        Some(config),
    )
    .await;

    Ok(stream)
}

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
async fn handshake(
    config: &WsConfig,
    upgrade: OnUpgrade,
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
        .serve(UpgradedIo::new(upgrade.await?)))
}

#[inline(always)]
fn configure<T>(listener: &mut Arc<Listener<T>>) -> &mut WsConfig {
    Arc::get_mut(listener)
        .map(|listener| &mut listener.config)
        .expect("cannot be configure ws while the app is running.")
}

impl<T> Ws<T> {
    pub(super) fn new(listener: T) -> Self {
        Self {
            listener: Arc::new(Listener {
                handle: listener,
                config: WsConfig::default(),
            }),
            guard: (
                guard::header(header::SEC_WEBSOCKET_VERSION, h::tag(b"13")),
                guard::header(header::CONNECTION, h::contains(h::tag_no_case(b"upgrade"))),
                guard::header(header::UPGRADE, h::contains(h::tag_no_case(b"websocket"))),
            ),
        }
    }

    /// The amount of memory to pre-allocate in bytes for buffered reads.
    ///
    /// **Default:** `16 KB`
    ///
    pub fn buffer_size(mut self, capacity: usize) -> Self {
        configure(&mut self.listener).buffer_size = capacity;
        self
    }

    /// The maximum size of a single incoming message frame.
    ///
    /// A `None` value indicates no frame size limit.
    ///
    /// **Default:** `16 KB`
    ///
    pub fn max_frame_size(mut self, limit: Option<usize>) -> Self {
        configure(&mut self.listener).max_frame_size = limit;
        self
    }

    /// The maximum message size in bytes.
    ///
    /// **Default:** `16 KB`
    ///
    pub fn max_message_size(mut self, limit: Option<usize>) -> Self {
        configure(&mut self.listener).max_message_size = limit;
        self
    }
}

impl<T> Ws<T> {
    fn verify(&self, headers: &HeaderMap) -> Result<Base64EncodedDigest, UpgradeError> {
        self.guard
            .cmp(headers)
            .map_err(|error| error.into())
            .and_then(|_| match headers.get(header::SEC_WEBSOCKET_KEY) {
                Some(value) => sha1(value.as_bytes()),
                None => Err(UpgradeError::SecWebsocketKey),
            })
    }
}

impl<T, App, Await> Middleware<App> for Ws<T>
where
    T: Fn(Channel, super::Request<App>) -> Await + Send + Sync + 'static,
    App: Send + Sync + 'static,
    Await: Future<Output = super::Result> + Send + 'static,
{
    fn call(&self, mut request: Request<App>, _: Next<App>) -> BoxFuture {
        let listener = Arc::clone(&self.listener);

        let Some(upgrade) = request.extensions_mut().remove::<OnUpgrade>() else {
            return Box::pin(async { deny!(500, UpgradeError::Other) });
        };

        let accept = match self.verify(request.headers()).or_bad_request() {
            Ok(digest) => digest,
            Err(error) => {
                return Box::pin(async { Err(error) });
            }
        };

        Box::pin(async move {
            let request = super::Request::new(request);

            tokio::spawn(async move {
                let stream = handshake(&listener.config, upgrade).await?;
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
