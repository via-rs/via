use std::ops::ControlFlow::Break;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::coop;

use crate::error::Error;

#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::protocol::{CloseFrame, Message, frame::Utf8Bytes};

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
pub use tokio_websockets::{CloseCode, Message};

pub struct Channel(Sender<Message>, Receiver<Message>);

impl Channel {
    pub(super) fn new() -> (Self, Self) {
        let (tx1, rx2) = mpsc::channel(1);
        let (tx2, rx1) = mpsc::channel(1);

        (Self(tx1, rx1), Self(tx2, rx2))
    }

    pub async fn send(&mut self, message: impl Into<Message>) -> super::Result<()> {
        self.0.send(message.into()).await.map_err(|_| {
            Break(Error::new(
                "channels cannot send messages after the receiving half is dropped",
            ))
        })
    }

    pub fn recv(&mut self) -> impl Future<Output = Option<Message>> {
        coop::unconstrained(self.1.recv())
    }
}
