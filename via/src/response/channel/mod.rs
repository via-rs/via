mod pipe;

pub(super) use pipe::PipeTask;

use bytes::Bytes;
use delegate::delegate;
use futures_channel::mpsc::SendError;
use futures_channel::{mpsc, oneshot};
use futures_core::Stream;
use http_body::{Body, Frame};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use crate::error::BoxError;

type RecvError = oneshot::Receiver<BoxError>;
type RecvFrame = mpsc::Receiver<Frame<Bytes>>;

pub struct ChannelBody {
    err: RecvError,
    rx: RecvFrame,
}

pub(super) struct Sender {
    err: Option<oneshot::Sender<BoxError>>,
    tx: mpsc::Sender<Frame<Bytes>>,
}

impl ChannelBody {
    #[inline]
    pub fn new() -> (Sender, Self) {
        let (etx, erx) = oneshot::channel();
        let (tx, rx) = mpsc::channel(0);
        let sender = Sender { err: Some(etx), tx };
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
        // The receiving halves of the channels in `self` are `Unpin`.
        //
        // Working with the inner mutable borrow behind `Pin<&mut Self>`
        // should result in the smallest number of derefs / reborrows.
        //
        // Exactly what we want for this particular use case.
        let this = self.get_mut();

        match Pin::new(&mut this.err).poll(context) {
            Poll::Pending => {
                // Poll the producer for the next frame.
                match ready!(Pin::new(&mut this.rx).poll_next(context)) {
                    Some(frame) => Poll::Ready(Some(Ok(frame))),
                    None => {
                        this.rx.close();
                        Poll::Ready(None)
                    }
                }
            }
            Poll::Ready(Ok(error)) => {
                // The producer errored.
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(Err(_)) => {
                // The producer exited.
                this.rx.close();
                Poll::Ready(None)
            }
        }
    }
}

impl Sender {
    delegate! {
        to self.tx {
            fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), SendError>>;
        }
    }

    fn send_frame(&mut self, frame: Frame<Bytes>) -> Result<(), SendError> {
        self.tx.start_send(frame)
    }

    fn send_error(&mut self, error: BoxError) -> Result<(), BoxError> {
        if let Some(tx) = self.err.take() {
            tx.send(error)
        } else {
            Err(error)
        }
    }
}
