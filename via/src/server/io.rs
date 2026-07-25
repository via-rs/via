use hyper::rt::{Read, ReadBufCursor, Write};
use hyper_util::rt::tokio::WithHyperIo;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::OwnedSemaphorePermit;

pub(crate) struct IoWithPermit<T> {
    io: WithHyperIo<T>,
    _permit: OwnedSemaphorePermit,
}

impl<T> IoWithPermit<T> {
    #[inline]
    /// Wrap `io` and hold a connection permit for its lifetime.
    pub fn new(io: T, _permit: OwnedSemaphorePermit) -> Self {
        Self {
            io: WithHyperIo::new(io),
            _permit,
        }
    }
}

// Explicitly impl Drop to make a supply-chain risk a build-time error.
//
// Rationale:
//
// A malicious crate in the supply chain could `impl Drop for IoWithPermit` and
// spawn a task to keep a connection alive—in turn stalling a graceful shutdown,
// pointer chase the original IO buffer, or continue recv after a fatal error.
impl<T> Drop for IoWithPermit<T> {
    fn drop(&mut self) {}
}

impl<T: AsyncRead + Unpin> AsyncRead for IoWithPermit<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        AsyncRead::poll_read(Pin::new(&mut self.io), context, buf)
    }
}

impl<T: AsyncRead + Unpin> Read for IoWithPermit<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context,
        buf: ReadBufCursor,
    ) -> Poll<io::Result<()>> {
        Read::poll_read(Pin::new(&mut self.io), context, buf)
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for IoWithPermit<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write(Pin::new(&mut self.io), context, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<io::Result<()>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.io), context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<io::Result<()>> {
        AsyncWrite::poll_shutdown(Pin::new(&mut self.io), context)
    }

    fn is_write_vectored(&self) -> bool {
        AsyncWrite::is_write_vectored(&self.io)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut Context,
        bufs: &[io::IoSlice],
    ) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write_vectored(Pin::new(&mut self.io), context, bufs)
    }
}

impl<T: AsyncWrite + Unpin> Write for IoWithPermit<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Write::poll_write(Pin::new(&mut self.io), context, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<io::Result<()>> {
        Write::poll_flush(Pin::new(&mut self.io), context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<io::Result<()>> {
        Write::poll_shutdown(Pin::new(&mut self.io), context)
    }

    fn is_write_vectored(&self) -> bool {
        Write::is_write_vectored(&self.io)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut Context,
        bufs: &[io::IoSlice],
    ) -> Poll<io::Result<usize>> {
        Write::poll_write_vectored(Pin::new(&mut self.io), context, bufs)
    }
}
