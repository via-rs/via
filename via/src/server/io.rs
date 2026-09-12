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
    pub fn new(io: T, _permit: OwnedSemaphorePermit) -> Self {
        Self {
            io: WithHyperIo::new(io),
            _permit,
        }
    }
}

impl<T> IoWithPermit<T> {
    #[inline(always)]
    fn project(self: Pin<&mut Self>) -> Pin<&mut WithHyperIo<T>> {
        // Safety:
        //
        // We need to project the `io` field in order to write forwarding impls
        // of `AsyncRead` and `AsyncWrite` for `Self`. `T` is usually `Unpin`.
        //
        // However, this wrapper type need not make assumptions about the
        // `Unpin`-ness of `T`. It only exists to keep the semaphore permit
        // live for the duration of the connection.
        //
        // Therefore, we use unsafe to project the `io` field with the smallest
        // number of instructions required to write the forwarding impls.
        //
        // The `Unpin`-ness of self is derived from `T` and the specific pin
        // requirements required by the canonical impls of `AsyncRead` and
        // `AsyncWrite` for `T` are handle by `T` because it owns the
        // allocation upholding these safety requirements.
        //
        // This is a trust boundary.
        unsafe { self.map_unchecked_mut(|this| &mut this.io) }
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

impl<T: AsyncRead> AsyncRead for IoWithPermit<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        AsyncRead::poll_read(self.project(), context, buf)
    }
}

impl<T: AsyncRead> Read for IoWithPermit<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context,
        buf: ReadBufCursor,
    ) -> Poll<io::Result<()>> {
        Read::poll_read(self.project(), context, buf)
    }
}

impl<T: AsyncWrite> AsyncWrite for IoWithPermit<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write(self.project(), context, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context) -> Poll<io::Result<()>> {
        AsyncWrite::poll_flush(self.project(), context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context) -> Poll<io::Result<()>> {
        AsyncWrite::poll_shutdown(self.project(), context)
    }

    fn is_write_vectored(&self) -> bool {
        AsyncWrite::is_write_vectored(&self.io)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        context: &mut Context,
        bufs: &[io::IoSlice],
    ) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write_vectored(self.project(), context, bufs)
    }
}

impl<T: AsyncWrite> Write for IoWithPermit<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Write::poll_write(self.project(), context, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context) -> Poll<io::Result<()>> {
        Write::poll_flush(self.project(), context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context) -> Poll<io::Result<()>> {
        Write::poll_shutdown(self.project(), context)
    }

    fn is_write_vectored(&self) -> bool {
        Write::is_write_vectored(&self.io)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        context: &mut Context,
        bufs: &[io::IoSlice],
    ) -> Poll<io::Result<usize>> {
        Write::poll_write_vectored(self.project(), context, bufs)
    }
}
