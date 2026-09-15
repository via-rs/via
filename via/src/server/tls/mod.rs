#[cfg(feature = "native-tls")]
mod native;

#[cfg(feature = "rustls-23")]
mod rustls;

#[cfg(feature = "native-tls")]
pub use native::NativeTlsAcceptor;

#[cfg(all(
    any(feature = "tokio-tungstenite", feature = "tokio-websockets"),
    not(feature = "rustls-23"),
    feature = "native-tls",
))]
pub use native::NativeTlsStream;

#[cfg(feature = "rustls-23")]
pub use rustls::RustlsAcceptor;

#[cfg(all(
    any(feature = "tokio-tungstenite", feature = "tokio-websockets"),
    not(feature = "native-tls"),
    feature = "rustls-23",
))]
pub use rustls::RustlsStream;

use http::Version;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::OwnedSemaphorePermit;

use crate::error::ServerError;
use crate::server::io::IoWithPermit;

type Result<T> = std::result::Result<IoWithPermit<T>, ServerError>;

pub(super) trait Acceptor {
    type Stream: AsyncRead + AsyncWrite + NegotiateAlpn;

    #[cfg_attr(
        not(any(feature = "native-tls", feature = "rustls-23")),
        allow(dead_code)
    )]
    fn accept(
        &self,
        stream: TcpStream,
        permit: OwnedSemaphorePermit,
    ) -> impl Future<Output = Result<Self::Stream>> + Send + 'static;
}

#[cfg_attr(
    not(any(feature = "native-tls", feature = "rustls-23")),
    allow(dead_code)
)]
pub trait NegotiateAlpn {
    fn preferred_alpn(&self) -> Alpn;
}

#[derive(Eq, PartialEq)]
pub struct Alpn(Version);

pub struct TcpAcceptor;

impl Acceptor for TcpAcceptor {
    type Stream = TcpStream;

    #[allow(clippy::manual_async_fn)]
    fn accept(
        &self,
        _: TcpStream,
        _: OwnedSemaphorePermit,
    ) -> impl Future<Output = Result<Self::Stream>> + Send + 'static {
        async { unreachable!() }
    }
}

impl NegotiateAlpn for TcpStream {
    fn preferred_alpn(&self) -> Alpn {
        unreachable!()
    }
}

#[allow(dead_code)]
impl Alpn {
    pub const HTTP_2: Self = Self(Version::HTTP_2);
    pub const HTTP_11: Self = Self(Version::HTTP_11);
}
