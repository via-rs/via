use http::StatusCode;
use http::header::{self as h, HeaderMap};
use std::future::Future;
use std::sync::Arc;
use tokio::task::coop::unconstrained;

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::protocol::{Role, WebSocketConfig},
};

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use super::io::UpgradedIo;
use super::run::RunTask;
use super::util::{Base64EncodedDigest, sha1};
use super::{Channel, Request};
use crate::guard::header::{CaseSensitive, Contains, DenyHeader, Header};
use crate::guard::{self, MapErr, Predicate, header};
use crate::ws::error::UpgradeError;
use crate::{BoxFuture, Error, Middleware, Next, Response, ResultExt};

const DEFAULT_FRAME_SIZE: usize = 16384; // 16KB

pub struct Ws<T> {
    listener: Arc<Listener<T>>,
    guard: MapErr<
        fn(DenyHeader<'_>) -> UpgradeError,
        (Header<CaseSensitive>, Header<Contains>, Header<Contains>),
    >,
}

pub(super) struct Listener<T> {
    pub(super) handle: T,
    config: WsConfig,
}

#[derive(Clone, Copy, Debug)]
struct WsConfig {
    buffer_size: usize,
    max_frame_size: Option<usize>,
    max_message_size: Option<usize>,
}

#[inline(always)]
fn configure<T>(listener: &mut Arc<Listener<T>>) -> &mut WsConfig {
    Arc::get_mut(listener)
        .map(|listener| &mut listener.config)
        .expect("cannot configure ws while the app is running.")
}

#[cfg(feature = "tokio-tungstenite")]
async fn handshake<App>(
    request: &mut Request<App>,
    config: &WsConfig,
) -> Result<WebSocketStream<UpgradedIo>, Error> {
    let on_upgrade = request.on_upgrade.take().expect("already upgraded.");
    let upgraded = UpgradedIo::new(on_upgrade.await?);
    let config = WebSocketConfig::default()
        .accept_unmasked_frames(false)
        .read_buffer_size(config.buffer_size)
        .write_buffer_size(config.buffer_size)
        .max_frame_size(config.max_frame_size)
        .max_message_size(config.max_message_size);

    Ok(WebSocketStream::from_raw_socket(upgraded, Role::Server, Some(config)).await)
}

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
async fn handshake<App>(
    request: &mut Request<App>,
    config: &WsConfig,
) -> Result<WebSocketStream<UpgradedIo>, Error> {
    use tokio_websockets::{Config, Limits, server::Builder};

    let on_upgrade = request.on_upgrade.take().expect("already upgraded.");
    let upgraded = UpgradedIo::new(on_upgrade.await?);
    let limits = Limits::default().max_payload_len(config.max_message_size);
    let config = Config::default()
        .frame_size(config.max_frame_size.unwrap_or(DEFAULT_FRAME_SIZE))
        .flush_threshold(config.buffer_size);

    Ok(Builder::new().config(config).limits(limits).serve(upgraded))
}

async fn reactor<T, App, Await>(mut request: Request<App>, listener: Arc<Listener<T>>)
where
    T: Fn(Channel, Request<App>) -> Await + Send,
    Await: Future<Output = super::Result> + Send + 'static,
{
    let err = match unconstrained(handshake(&mut request, &listener.config)).await {
        Err(error) => Some(error),
        Ok(stream) => {
            let task = RunTask::new(listener, request, stream);
            task.await.err()
        }
    };

    if let Some(error) = err.as_ref() {
        #[cfg(not(debug_assertions))]
        let _ = error;

        log!(error(0), "{}", error);
    }
}

impl<T> Ws<T> {
    pub(super) fn new(listener: T) -> Self {
        Self {
            listener: Arc::new(Listener {
                handle: listener,
                config: WsConfig {
                    buffer_size: DEFAULT_FRAME_SIZE,
                    max_frame_size: Some(DEFAULT_FRAME_SIZE),
                    max_message_size: Some(DEFAULT_FRAME_SIZE),
                },
            }),
            guard: guard::map_err(
                |error| error.into(),
                (
                    header(h::SEC_WEBSOCKET_VERSION, header::case_sensitive(b"13")),
                    header(h::CONNECTION, header::contains(header::tag(b"upgrade"))),
                    header(h::UPGRADE, header::contains(header::tag(b"websocket"))),
                ),
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
    #[inline]
    fn validate(&self, headers: &HeaderMap) -> Result<Base64EncodedDigest, UpgradeError> {
        self.guard
            .cmp(headers)
            .and_then(|_| match headers.get(h::SEC_WEBSOCKET_KEY) {
                Some(key) => sha1(key.as_bytes()),
                None => Err(UpgradeError::SecWebsocketKey),
            })
    }
}

impl<T, App, Await> Middleware<App> for Ws<T>
where
    T: Fn(Channel, Request<App>) -> Await + Send + Sync + 'static,
    App: Send + Sync + 'static,
    Await: Future<Output = super::Result> + Send + 'static,
{
    fn call(&self, request: crate::Request<App>, _: Next<App>) -> BoxFuture {
        let listener = Arc::clone(&self.listener);
        let is_valid = self.validate(request.headers());

        Box::pin(async {
            let accept = is_valid.or_bad_request()?;
            let result = Response::build()
                .status(StatusCode::SWITCHING_PROTOCOLS)
                .header(h::CONNECTION, "upgrade")
                .header(h::SEC_WEBSOCKET_ACCEPT, accept.as_str())
                .header(h::UPGRADE, "websocket")
                .finish();

            tokio::spawn(reactor(Request::new(request), listener));

            result
        })
    }
}
