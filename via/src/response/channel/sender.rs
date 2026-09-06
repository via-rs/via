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
        to self.sender {
            pub(super) fn close_channel(&mut self);
            pub(super) fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), SendError>>;
            pub(super) fn send_frame(&mut self, frame: Frame<Bytes>) -> Result<(), SendError>;
            pub(super) fn send_error(&mut self, error: BoxError) -> Result<(), BoxError>;

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
