use bytes::Bytes;
use delegate::delegate;
use futures_channel::mpsc::SendError;
use futures_channel::{mpsc, oneshot};
use http_body::Frame;
use std::task::{Context, Poll};

use crate::error::BoxError;

pub struct Sender {
    sender: SenderImpl,
}

struct SenderImpl {
    err: Option<oneshot::Sender<BoxError>>,
    tx: mpsc::Sender<Frame<Bytes>>,
}

impl Sender {
    pub(super) fn new(err: oneshot::Sender<BoxError>, tx: mpsc::Sender<Frame<Bytes>>) -> Self {
        Self {
            sender: SenderImpl { err: Some(err), tx },
        }
    }

    delegate! {
        to self.sender.tx {
            pub(super) fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), SendError>>;
        }
    }

    pub(super) fn send_frame(&mut self, frame: Frame<Bytes>) -> Result<(), SendError> {
        self.sender.tx.start_send(frame)
    }

    pub(super) fn send_error(&mut self, error: BoxError) -> Result<(), BoxError> {
        if let Some(tx) = self.sender.err.take() {
            tx.send(error)
        } else {
            Err(error)
        }
    }
}
