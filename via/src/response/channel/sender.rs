use bytes::Bytes;
use delegate::delegate;
use futures_channel::mpsc::SendError;
use futures_channel::{mpsc, oneshot};
use http_body::Frame;
use std::task::{Context, Poll};

use crate::error::BoxError;

/// The sending half of the channel created by
/// [`ResponseBody::channel`](crate::response::ResponseBody::channel).
pub struct Sender {
    sender: SenderImpl,
}

struct SenderImpl {
    err: Option<oneshot::Sender<BoxError>>,
    tx: mpsc::Sender<Frame<Bytes>>,
}

/// Documentation sourced from [`futures_channel::mpsc::Sender`].
impl Sender {
    delegate! {
        to self.sender {
            /// Closes this channel from the sender side, preventing any new
            /// messages.
            pub fn close_channel(&mut self);

            /// Polls the channel to determine if there is guaranteed capacity
            /// to send at least one item without waiting.
            pub fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), SendError>>;

            /// Send a message on the channel.
            ///
            /// This function should only be called after `Poll::Ready(_)` is
            /// returned by [`Self::poll_ready`].
            pub fn send_frame(&mut self, frame: Frame<Bytes>) -> Result<(), SendError>;

            /// Close the channel from the sender side with the provided
            /// `error`, preventing any new messages.
            pub fn send_error(&mut self, error: BoxError) -> Result<(), BoxError>;
        }
    }
}

impl Sender {
    pub(super) fn new(err: oneshot::Sender<BoxError>, tx: mpsc::Sender<Frame<Bytes>>) -> Self {
        Self {
            sender: SenderImpl { err: Some(err), tx },
        }
    }
}

impl SenderImpl {
    delegate! {
        to self.tx {
            fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), SendError>>;
        }
    }

    fn close_channel(&mut self) {
        self.tx.close_channel();
        self.err = None;
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
