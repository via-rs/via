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
use std::convert::Infallible;
use std::error::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::OwnedSemaphorePermit;

use super::io::IoWithPermit;

pub(super) trait Acceptor {
    type Error: Error;
    type Stream: AsyncRead + AsyncWrite + NegotiateAlpn;

    #[cfg_attr(
        not(any(feature = "native-tls", feature = "rustls-23")),
        allow(dead_code)
    )]
    fn accept(
        &self,
        permit: OwnedSemaphorePermit,
        stream: TcpStream,
    ) -> impl Future<Output = Result<IoWithPermit<Self::Stream>, Self::Error>> + Send + 'static;
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
    type Error = Infallible;
    type Stream = TcpStream;

    #[allow(clippy::manual_async_fn)]
    fn accept(
        &self,
        _: OwnedSemaphorePermit,
        _: TcpStream,
    ) -> impl Future<Output = Result<IoWithPermit<Self::Stream>, Self::Error>> + Send + 'static
    {
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
