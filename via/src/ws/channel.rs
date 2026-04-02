use futures_core::Stream;
use loole::{RecvStream, Sender};
use std::future::{Future, poll_fn};
use std::ops::ControlFlow;
use std::pin::Pin;

use crate::error::Error;

#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::protocol::{CloseFrame, Message, frame::Utf8Bytes};

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
pub use tokio_websockets::{CloseCode, Message};

pub struct Channel(Sender<Message>, RecvStream<Message>);

impl Channel {
    pub async fn send(&mut self, message: impl Into<Message>) -> super::Result<()> {
        let Ok(_) = self.0.send_async(message.into()).await else {
            let message = "failed to send ws message. channel disconnected.";
            return Err(ControlFlow::Break(Error::new(message)));
        };

        Ok(())
    }

    pub fn recv(&mut self) -> impl Future<Output = Option<Message>> {
        poll_fn(|context| Pin::new(&mut self.1).poll_next(context))
    }
}

impl Channel {
    pub(super) fn new() -> (Self, Self) {
        let (tx1, rx2) = loole::bounded(0);
        let (tx2, rx1) = loole::bounded(0);

        (Self(tx1, rx1.into_stream()), Self(tx2, rx2.into_stream()))
    }
}
