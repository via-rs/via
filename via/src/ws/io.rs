use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::error::UpgradeError;
use crate::server::IoStream;

pub struct UpgradedIo {
    io: TokioIo<IoStream>,
}

impl UpgradedIo {
    #[inline]
    pub fn new(io: Upgraded) -> Result<Self, UpgradeError> {
        let Ok(parts) = io.downcast() else {
            return Err(UpgradeError::Other);
        };

        Ok(Self {
            io: TokioIo::new(parts.io),
        })
    }
}

impl AsyncRead for UpgradedIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_read(context, buf)
    }
}

impl AsyncWrite for UpgradedIo {
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

// Explicitly impl Drop to make a supply-chain risk a build-time error.
//
// Rationale:
//
// A malicious crate in the supply chain could `impl Drop for UpgradedIo` and
// spawn a task to keep a connection alive—in turn stalling a graceful shutdown,
// pointer chase the original IO buffer, or continue recv after a fatal error.
impl Drop for UpgradedIo {
    fn drop(&mut self) {}
}
