use rustls::ServerConfig;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::server::{Accept, TlsAcceptor, TlsStream};

use super::{Acceptor, Alpn, NegotiateAlpn};

pub struct RustlsAcceptor(TlsAcceptor);

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct RustlsStream {
    alpn: Alpn,
    tls: Pin<Box<MaybeTlsStream>>,
}

enum ReadyState {
    Handshake(Accept<TcpStream>),
    Stream(TlsStream<TcpStream>),
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
struct MaybeTlsStream {
    state: ReadyState,
}

impl RustlsAcceptor {
    pub fn new(rustls_config: ServerConfig) -> Self {
        Self(TlsAcceptor::from(Arc::new(rustls_config)))
    }
}

impl Acceptor for RustlsAcceptor {
    type Error = io::Error;
    type Stream = RustlsStream;

    fn accept(
        &self,
        io: TcpStream,
    ) -> impl Future<Output = Result<Self::Stream, Self::Error>> + Send + 'static {
        let acceptor = self.0.clone();

        async move {
            let mut tls = Box::pin(MaybeTlsStream {
                state: ReadyState::Handshake(acceptor.accept(io)),
            });

            let alpn = (&mut tls).await?;

            Ok(RustlsStream { alpn, tls })
        }
    }
}

impl MaybeTlsStream {
    #[inline]
    fn with_stream<F, T>(self: Pin<&mut Self>, f: F) -> Poll<io::Result<T>>
    where
        F: FnOnce(Pin<&mut TlsStream<TcpStream>>) -> Poll<io::Result<T>>,
    {
        let this = self.get_mut();

        if let ReadyState::Stream(stream) = &mut this.state {
            let stream = Pin::new(stream);
            f(stream)
        } else {
            Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))
        }
    }
}

impl AsyncRead for MaybeTlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        self.with_stream(|stream| stream.poll_read(cx, buf))
    }
}

impl AsyncWrite for MaybeTlsStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.with_stream(|stream| stream.poll_write(cx, buf))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.with_stream(|stream| stream.poll_flush(cx))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.with_stream(|stream| stream.poll_shutdown(cx))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &[io::IoSlice],
    ) -> Poll<io::Result<usize>> {
        self.with_stream(|stream| stream.poll_write_vectored(cx, bufs))
    }
}

impl Future for MaybeTlsStream {
    type Output = io::Result<Alpn>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let ReadyState::Handshake(accept) = &mut this.state else {
            return Poll::Ready(Err(io::ErrorKind::AlreadyExists.into()));
        };

        Pin::new(accept).poll(cx).map_ok(|stream| {
            let (_, conn) = stream.get_ref();
            let alpn = match conn.alpn_protocol() {
                Some(value) if value == b"h2" => Alpn::HTTP_2,
                _ => Alpn::HTTP_11,
            };

            this.state = ReadyState::Stream(stream);

            alpn
        })
    }
}

impl RustlsStream {
    #[inline(always)]
    fn project(self: Pin<&mut Self>) -> Pin<&mut MaybeTlsStream> {
        let this = self.get_mut();
        this.tls.as_mut()
    }
}

impl AsyncRead for RustlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        self.project().poll_read(cx, buf)
    }
}

impl AsyncWrite for RustlsStream {
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
        self.tls.is_write_vectored()
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &[io::IoSlice],
    ) -> Poll<io::Result<usize>> {
        self.project().poll_write_vectored(cx, bufs)
    }
}

impl NegotiateAlpn for RustlsStream {
    fn preferred_alpn(&self) -> &Alpn {
        &self.alpn
    }
}
