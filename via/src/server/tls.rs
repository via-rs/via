use http::Version;
use std::error::Error;
use std::io;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

#[cfg(feature = "native-tls")]
pub use native::NativeTlsAcceptor;

#[cfg(feature = "rustls-23")]
pub use rustls::RustlsAcceptor;

#[derive(Eq, PartialEq)]
pub struct Alpn(Version);

pub struct TcpAcceptor;

pub trait Acceptor {
    type Io: AsyncRead + AsyncWrite;
    type Error: Error + Send;

    #[cfg_attr(
        not(any(feature = "native-tls", feature = "rustls-23")),
        allow(dead_code)
    )]
    fn accept(
        &self,
        io: TcpStream,
    ) -> impl Future<Output = Result<(Self::Io, Alpn), Self::Error>> + Send + 'static;
}

impl Acceptor for TcpAcceptor {
    type Io = TcpStream;
    type Error = io::Error;

    #[allow(clippy::manual_async_fn)]
    fn accept(
        &self,
        _: TcpStream,
    ) -> impl Future<Output = Result<(Self::Io, Alpn), Self::Error>> + Send + 'static {
        async { unreachable!() }
    }
}

#[allow(dead_code)]
impl Alpn {
    pub const HTTP_2: Self = Self(Version::HTTP_2);
    pub const HTTP_11: Self = Self(Version::HTTP_11);
}

#[cfg(feature = "native-tls")]
mod native {
    use native_tls::{Identity, Protocol};
    use std::sync::Arc;
    use tokio::net::TcpStream;
    use tokio_native_tls::{TlsAcceptor, TlsStream};

    use super::{Acceptor, Alpn};

    pub struct NativeTlsAcceptor(Arc<TlsAcceptor>);

    impl NativeTlsAcceptor {
        pub fn new(identity: Identity) -> Self {
            Self(Arc::new(TlsAcceptor::from(
                native_tls::TlsAcceptor::builder(identity)
                    .min_protocol_version(Some(Protocol::Tlsv12))
                    .build()
                    .expect("tls config is invalid or missing"),
            )))
        }
    }

    impl Acceptor for NativeTlsAcceptor {
        type Io = TlsStream<TcpStream>;
        type Error = native_tls::Error;

        fn accept(
            &self,
            io: TcpStream,
        ) -> impl Future<Output = Result<(Self::Io, Alpn), Self::Error>> + Send + 'static {
            let acceptor = Arc::clone(&self.0);

            async move {
                let io = acceptor.accept(io).await?;
                let stream = io.get_ref();
                let alpn = stream
                    .negotiated_alpn()?
                    .and_then(|negotiated| (negotiated == b"h2").then_some(Alpn::HTTP_2))
                    .unwrap_or(Alpn::HTTP_11);

                Ok((io, alpn))
            }
        }
    }
}

#[cfg(feature = "rustls-23")]
mod rustls {
    use rustls::ServerConfig;
    use std::{io, sync::Arc};
    use tokio::net::TcpStream;
    use tokio_rustls::server::{TlsAcceptor, TlsStream};

    use super::{Acceptor, Alpn};

    pub struct RustlsAcceptor(TlsAcceptor);

    impl RustlsAcceptor {
        pub fn new(rustls_config: ServerConfig) -> Self {
            Self(TlsAcceptor::from(Arc::new(rustls_config)))
        }
    }

    impl Acceptor for RustlsAcceptor {
        type Io = TlsStream<TcpStream>;
        type Error = io::Error;

        fn accept(
            &self,
            io: TcpStream,
        ) -> impl Future<Output = Result<(Self::Io, Alpn), Self::Error>> + Send + 'static {
            let future = self.0.accept(io);

            async move {
                let io = future.await?;
                let (_, conn) = io.get_ref();
                let alpn = conn
                    .alpn_protocol()
                    .and_then(|negotiated| (negotiated == b"h2").then_some(Alpn::HTTP_2))
                    .unwrap_or(Alpn::HTTP_11);

                Ok((io, alpn))
            }
        }
    }
}
