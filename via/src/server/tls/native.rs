use native_tls::{Identity, Protocol};
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::timeout;
use tokio_native_tls::{TlsAcceptor, TlsStream};

use super::{Acceptor, Alpn, NegotiateAlpn};
use crate::server::io::IoWithPermit;

pub struct NativeTlsAcceptor {
    acceptor: Arc<AcceptorImpl>,
}

pub struct NativeTlsStream {
    stream: TlsStream<TcpStream>,
}

struct AcceptorImpl {
    native_tls: TlsAcceptor,
    timeout: Duration,
}

impl NativeTlsAcceptor {
    pub fn new(identity: Identity, timeout: Duration, alpn_protocols: &[impl AsRef<str>]) -> Self {
        let native_tls = TlsAcceptor::from(
            native_tls::TlsAcceptor::builder(identity)
                .min_protocol_version(Some(Protocol::Tlsv12))
                .accept_alpn(alpn_protocols)
                .build()
                .expect("tls config is invalid or missing"),
        );

        Self {
            acceptor: Arc::new(AcceptorImpl {
                native_tls,
                timeout,
            }),
        }
    }
}

impl Acceptor for NativeTlsAcceptor {
    type Stream = NativeTlsStream;

    fn accept(
        &self,
        stream: TcpStream,
        permit: OwnedSemaphorePermit,
    ) -> impl Future<Output = io::Result<IoWithPermit<Self::Stream>>> + Send + 'static {
        let acceptor = Arc::clone(&self.acceptor);

        async move {
            match timeout(acceptor.timeout, acceptor.native_tls.accept(stream)).await {
                Ok(Ok(stream)) => Ok(IoWithPermit::new(NativeTlsStream { stream }, permit)),
                Ok(Err(error)) => Err(io::Error::other(error)),
                Err(_) => Err(io::Error::from(io::ErrorKind::TimedOut)),
            }
        }
    }
}

impl NativeTlsStream {
    #[inline(always)]
    fn project(self: Pin<&mut Self>) -> Pin<&mut TlsStream<TcpStream>> {
        // Reify the borrow immediately before crossing the FFI boundary.
        let this = &mut *self.get_mut();

        // Return the projection.
        Pin::new(&mut this.stream)
    }
}

impl AsyncRead for NativeTlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        self.project().poll_read(cx, buf)
    }
}

impl AsyncWrite for NativeTlsStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.project().poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.project().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.project().poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        false // native-tls does not currently support vectored writes.
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &[io::IoSlice],
    ) -> Poll<io::Result<usize>> {
        self.project().poll_write_vectored(cx, bufs)
    }
}

impl NegotiateAlpn for NativeTlsStream {
    fn preferred_alpn(&self) -> Alpn {
        match self.stream.get_ref().negotiated_alpn() {
            Ok(Some(ref alpn)) if alpn == b"h2" => Alpn::HTTP_2,
            Ok(Some(_) | None) => Alpn::HTTP_11,
            Err(_) => {
                std::hint::cold_path();
                Alpn::HTTP_11
            }
        }
    }
}
