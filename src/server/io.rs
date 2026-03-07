use hyper::rt::{Read, ReadBufCursor, Write};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::OwnedSemaphorePermit;

pub(crate) struct IoWithPermit<T> {
    io: T,
    _permit: OwnedSemaphorePermit,
}

impl<T> IoWithPermit<T> {
    #[inline]
    pub fn new(io: T, _permit: OwnedSemaphorePermit) -> Self {
        Self { io, _permit }
    }
}

impl<T: AsyncRead + Unpin> Read for IoWithPermit<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context,
        mut buf: ReadBufCursor,
    ) -> Poll<io::Result<()>> {
        // Safety:
        //
        // ReadBufCursor::as_mut requires that no bytes are uninitialized that
        // have been initialized before. We trust that the impl of T::poll_read
        // reads linearly into the unfilled portion of buf and that the filled
        // portion of buf ends at the byte position of the last byte that was
        // read during the call to <T as AsyncRead>::poll_read.
        let len = unsafe {
            let mut dest = ReadBuf::uninit(buf.as_mut());
            let Poll::Ready(_) = Pin::new(&mut self.io).poll_read(context, &mut dest)? else {
                return Poll::Pending;
            };

            dest.filled().len()
        };

        // Safety:
        //
        // The buffer is advanced by the exact number of bytes that were read
        // during the call to <T as AsyncRead>::poll_read. We trust that the
        // implementation of TcpStream for the runtime is sound and does not
        // grow the filled portion of the buffer beyond it's total size.
        //
        // In other words, we're looking at the answer to the math problem we
        // are stuck on that the person we are sitting next to wrote down. They
        // always get it right.
        unsafe { buf.advance(len) };

        Poll::Ready(Ok(()))
    }
}

impl<T: AsyncWrite + Unpin> Write for IoWithPermit<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(context, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(context)
    }

    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write_vectored(context, bufs)
    }
}
