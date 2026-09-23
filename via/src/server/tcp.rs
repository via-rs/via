use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::OwnedSemaphorePermit;

use crate::server::io::IoWithPermit;
use crate::server::tls::{Acceptor, Alpn, NegotiateAlpn};

pub struct TcpAcceptor {
    _timeout: Duration,
}

pub struct TcpStream {
    io: Pin<Box<tokio::net::TcpStream>>,
}

impl TcpAcceptor {
    pub(super) fn new(_timeout: Duration) -> Self {
        Self { _timeout }
    }
}

impl Acceptor for TcpAcceptor {
    type Stream = TcpStream;

    fn accept(
        &self,
        stream: tokio::net::TcpStream,
        permit: OwnedSemaphorePermit,
    ) -> impl Future<Output = io::Result<IoWithPermit<Self::Stream>>> + Send + 'static {
        async move { Ok(IoWithPermit::new(TcpStream::new(stream), permit)) }
    }
}

impl NegotiateAlpn for TcpStream {
    #[inline]
    fn preferred_alpn(&self) -> Alpn {
        Alpn::HTTP_11
    }
}

impl TcpStream {
    pub(super) fn new(io: tokio::net::TcpStream) -> Self {
        Self { io: Box::pin(io) }
    }
}

impl TcpStream {
    #[inline(always)]
    fn project(self: Pin<&mut Self>) -> Pin<&mut tokio::net::TcpStream> {
        let this = self.get_mut();
        this.io.as_mut()
    }
}

impl AsyncRead for TcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        self.project().poll_read(context, buf)
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().poll_write(context, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context) -> Poll<io::Result<()>> {
        self.project().poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context) -> Poll<io::Result<()>> {
        self.project().poll_shutdown(context)
    }

    fn is_write_vectored(&self) -> bool {
        false // tcp streams do not currently support vectored writes.
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        context: &mut Context,
        bufs: &[io::IoSlice],
    ) -> Poll<io::Result<usize>> {
        self.project().poll_write_vectored(context, bufs)
    }
}
