#[cfg(feature = "native-tls")]
mod native;

#[cfg(feature = "rustls-23")]
mod rustls;

#[cfg(feature = "native-tls")]
/// TLS acceptor backed by `native-tls`.
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
use std::convert::Infallible;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use crate::error::ServerError;

/// Accepts a TCP stream and optionally upgrades it to a TLS stream.
pub trait Acceptor {
    /// Error returned when accepting a stream fails.
    type Error: Into<ServerError>;
    /// Stream type produced by the acceptor.
    type Stream: AsyncRead + AsyncWrite + NegotiateAlpn;

    #[cfg_attr(
        not(any(feature = "native-tls", feature = "rustls-23")),
        allow(dead_code)
    )]
    /// Accept an inbound TCP stream.
    fn accept(
        &self,
        io: TcpStream,
    ) -> impl Future<Output = Result<Self::Stream, Self::Error>> + Send + 'static;
}

#[cfg_attr(
    not(any(feature = "native-tls", feature = "rustls-23")),
    allow(dead_code)
)]
/// Reports the application protocol negotiated for a stream.
pub trait NegotiateAlpn {
    /// Return the preferred or negotiated ALPN protocol.
    fn preferred_alpn(&self) -> &Alpn;
}

#[derive(Eq, PartialEq)]
/// Application-layer protocol negotiated for a connection.
pub struct Alpn(Version);

/// Plain TCP acceptor used when TLS is disabled.
pub struct TcpAcceptor;

impl Acceptor for TcpAcceptor {
    type Error = Infallible;
    type Stream = TcpStream;

    #[allow(clippy::manual_async_fn)]
    /// Accept an inbound TCP stream.
    fn accept(
        &self,
        _: TcpStream,
    ) -> impl Future<Output = Result<Self::Stream, Self::Error>> + Send + 'static {
        async { unreachable!() }
    }
}

impl NegotiateAlpn for TcpStream {
    fn preferred_alpn(&self) -> &Alpn {
        unreachable!()
    }
}

#[allow(dead_code)]
impl Alpn {
    /// HTTP/2 ALPN marker.
    pub const HTTP_2: Self = Self(Version::HTTP_2);
    /// HTTP/1.1 ALPN marker.
    pub const HTTP_11: Self = Self(Version::HTTP_11);
}
