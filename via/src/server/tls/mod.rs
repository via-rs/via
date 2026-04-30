#[cfg(feature = "native-tls")]
mod native;

#[cfg(feature = "rustls-23")]
mod rustls;

#[cfg(feature = "native-tls")]
pub use native::{NativeTlsAcceptor, NativeTlsStream};

#[cfg(feature = "rustls-23")]
pub use rustls::{RustlsAcceptor, RustlsStream};

use http::Version;
use std::convert::Infallible;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use crate::error::ServerError;

pub trait Acceptor {
    type Error: Into<ServerError>;
    type Stream: AsyncRead + AsyncWrite + NegotiateAlpn;

    #[cfg_attr(
        not(any(feature = "native-tls", feature = "rustls-23")),
        allow(dead_code)
    )]
    fn accept(
        &self,
        io: TcpStream,
    ) -> impl Future<Output = Result<Self::Stream, Self::Error>> + Send + 'static;
}

#[cfg_attr(
    not(any(feature = "native-tls", feature = "rustls-23")),
    allow(dead_code)
)]
pub trait NegotiateAlpn {
    fn preferred_alpn(&self) -> &Alpn;
}

#[derive(Eq, PartialEq)]
pub struct Alpn(Version);

pub struct TcpAcceptor;

impl Acceptor for TcpAcceptor {
    type Error = Infallible;
    type Stream = TcpStream;

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
    pub const HTTP_2: Self = Self(Version::HTTP_2);
    pub const HTTP_11: Self = Self(Version::HTTP_11);
}
