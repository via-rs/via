mod pipe;
mod sender;

pub(super) use pipe::PipeTask;
pub use sender::Sender;

use bytes::Bytes;
use futures_channel::{mpsc, oneshot};
use futures_core::Stream;
use http_body::{Body, Frame};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::error::BoxError;

pub struct ChannelBody {
    err: oneshot::Receiver<BoxError>,
    rx: mpsc::Receiver<Frame<Bytes>>,
}

impl ChannelBody {
    #[inline]
    pub fn new() -> (Sender, Self) {
        let (etx, erx) = oneshot::channel();
        let (tx, rx) = mpsc::channel(0);
        let sender = Sender::new(etx, tx);
        let body = Self { rx, err: erx };

        (sender, body)
    }
}

impl Body for ChannelBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        context: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        // `self` can only meaningfully exist in an `UnsyncBoxBody`.
        let this = self.get_mut();

        // Poll the `err` channel first. It will also propagate a dropped sender.
        if let Poll::Ready(result) = Pin::new(&mut this.err).poll(context) {
            if let Ok(error) = result {
                this.rx.close(); // Eagerly close the channel.
                Poll::Ready(Some(Err(error)))
            } else {
                Poll::Ready(None)
            }
        } else {
            match Pin::new(&mut this.rx).poll_next(context) {
                Poll::Ready(Some(frame)) => Poll::Ready(Some(Ok(frame))),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}
