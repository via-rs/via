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
use std::io;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::OwnedSemaphorePermit;

use super::io::IoWithPermit;

#[derive(Eq, PartialEq)]
pub(crate) struct Alpn(Version);

pub(crate) trait Acceptor {
    type Stream: AsyncRead + AsyncWrite + NegotiateAlpn;

    #[cfg_attr(
        not(any(feature = "native-tls", feature = "rustls-23")),
        allow(dead_code)
    )]
    fn accept(
        &self,
        stream: TcpStream,
        permit: OwnedSemaphorePermit,
    ) -> impl Future<Output = io::Result<IoWithPermit<Self::Stream>>> + Send + 'static;
}

pub(crate) trait NegotiateAlpn {
    fn preferred_alpn(&self) -> Alpn;
}

impl Alpn {
    pub(crate) const HTTP_2: Self = Self(Version::HTTP_2);
    pub(crate) const HTTP_11: Self = Self(Version::HTTP_11);
}
