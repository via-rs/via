use native_tls::{Identity, Protocol};
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_native_tls::{TlsAcceptor, TlsStream};

use crate::server::tls::NegotiateAlpn;

use super::{Acceptor, Alpn};

pub struct NativeTlsAcceptor(Arc<TlsAcceptor>);

pub struct NativeTlsStream {
    alpn: Alpn,
    stream: TlsStream<TcpStream>,
}

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
    type Stream = NativeTlsStream;
    type Error = native_tls::Error;

    fn accept(
        &self,
        io: TcpStream,
    ) -> impl Future<Output = Result<Self::Stream, Self::Error>> + Send + 'static {
        let acceptor = Arc::clone(&self.0);

        async move {
            let stream = acceptor.accept(io).await?;
            let alpn = stream
                .get_ref()
                .negotiated_alpn()?
                .and_then(|negotiated| (negotiated == b"h2").then_some(Alpn::HTTP_2))
                .unwrap_or(Alpn::HTTP_11);

            Ok(NativeTlsStream { alpn, stream })
        }
    }
}

impl AsyncRead for NativeTlsStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for NativeTlsStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &[io::IoSlice],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write_vectored(cx, bufs)
    }
}

impl NegotiateAlpn for NativeTlsStream {
    fn preferred_alpn(&self) -> &Alpn {
        &self.alpn
    }
}
