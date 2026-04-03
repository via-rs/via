use futures_channel::mpsc::{self, Receiver, Sender};
use futures_core::FusedFuture;
use std::future::Future;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::error::Error;

#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::protocol::{CloseFrame, Message, frame::Utf8Bytes};

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
pub use tokio_websockets::{CloseCode, Message};

pub struct Channel {
    tx: Sender<Message>,
    rx: Receiver<Message>,
}

struct Send<'a> {
    tx: &'a mut Sender<Message>,
    message: Option<Message>,
}

fn disconnect_error() -> ControlFlow<Error, Error> {
    ControlFlow::Break(Error::new(
        "failed to send ws message. channel disconnected.",
    ))
}

impl Channel {
    pub fn send(&mut self, message: impl Into<Message>) -> impl Future<Output = super::Result> {
        Send {
            tx: &mut self.tx,
            message: Some(message.into()),
        }
    }

    pub async fn recv(&mut self) -> Option<Message> {
        self.rx.recv().await.ok()
    }
}

impl Channel {
    pub(super) fn new() -> (Self, Self) {
        let (tx1, rx2) = mpsc::channel(1);
        let (tx2, rx1) = mpsc::channel(1);

        (Self { tx: tx1, rx: rx1 }, Self { tx: tx2, rx: rx2 })
    }

    pub(super) fn rx(&mut self) -> &mut Receiver<Message> {
        &mut self.rx
    }

    pub(super) fn tx(&mut self) -> &mut Sender<Message> {
        &mut self.tx
    }
}

impl Future for Send<'_> {
    type Output = super::Result;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        if let Some(message) = self.message.take() {
            let Poll::Ready(_) = Pin::new(&mut self.tx)
                .poll_ready(context)
                .map_err(|_| disconnect_error())?
            else {
                self.message = Some(message);
                return Poll::Pending;
            };

            self.tx.try_send(message).map_err(|_| disconnect_error())?;
        }

        Poll::Ready(Ok(()))
    }
}

impl FusedFuture for Send<'_> {
    fn is_terminated(&self) -> bool {
        self.message.is_none()
    }
}
