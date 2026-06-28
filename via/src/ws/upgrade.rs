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

use super::run::RunTask;
use super::util::{Base64EncodedDigest, sha1};
use super::{Channel, Request};
use crate::guard::bytes::{CaseSensitive, Contains, Tag, Trim};
use crate::guard::{Header, Predicate, header};
use crate::server::IoStream;
use crate::ws::error::UpgradeError;
use crate::{BoxFuture, Error, Middleware, Next, Response, ResultExt};

const DEFAULT_FRAME_SIZE: usize = 16384; // 16KB

type HasToken = Contains<Trim<Tag>>;

pub struct Ws<T> {
    listener: Arc<Listener<T>>,
    guard: (Header<CaseSensitive>, Header<HasToken>, Header<HasToken>),
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
) -> Result<WebSocketStream<IoStream>, Error> {
    let on_upgrade = request.on_upgrade.take().expect("already upgraded.");
    let config = WebSocketConfig::default()
        .accept_unmasked_frames(false)
        .read_buffer_size(config.buffer_size)
        .write_buffer_size(config.buffer_size)
        .max_frame_size(config.max_frame_size)
        .max_message_size(config.max_message_size);

    let upgraded = on_upgrade.await?;
    let Ok(parts) = upgraded.downcast() else {
        return Err(UpgradeError::Other.into());
    };

    Ok(WebSocketStream::from_raw_socket(parts.io, Role::Server, Some(config)).await)
}

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
async fn handshake<App>(
    request: &mut Request<App>,
    config: &WsConfig,
) -> Result<WebSocketStream<IoStream>, Error> {
    use tokio_websockets::{Config, Limits, server::Builder};

    let on_upgrade = request.on_upgrade.take().expect("already upgraded.");
    let limits = Limits::default().max_payload_len(config.max_message_size);
    let config = Config::default()
        .frame_size(config.max_frame_size.unwrap_or(DEFAULT_FRAME_SIZE))
        .flush_threshold(config.buffer_size);

    let upgraded = on_upgrade.await?;
    let Ok(parts) = upgraded.downcast() else {
        return Err(UpgradeError::Other.into());
    };

    Ok(Builder::new().config(config).limits(limits).serve(parts.io))
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
        use crate::guard::bytes::{case_sensitive, contains, tag, trim};

        Self {
            listener: Arc::new(Listener {
                handle: listener,
                config: WsConfig {
                    buffer_size: DEFAULT_FRAME_SIZE,
                    max_frame_size: Some(DEFAULT_FRAME_SIZE),
                    max_message_size: Some(DEFAULT_FRAME_SIZE),
                },
            }),
            guard: (
                header(h::SEC_WEBSOCKET_VERSION, case_sensitive(b"13")),
                header(h::CONNECTION, contains(trim(tag(b"upgrade")), b',')),
                header(h::UPGRADE, contains(trim(tag(b"websocket")), b',')),
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
        self.guard.cmp(headers).map_or_else(
            |error| Err(error.into()),
            |_| match headers.get(h::SEC_WEBSOCKET_KEY) {
                Some(key) => sha1(key.as_bytes()),
                None => Err(UpgradeError::SecWebsocketKey),
            },
        )
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
                .header(h::SEC_WEBSOCKET_ACCEPT, accept.as_str()?)
                .header(h::UPGRADE, "websocket")
                .finish();

            tokio::spawn(reactor(Request::new(request), listener));

            result
        })
    }
}
