use std::ops::ControlFlow;
use tokio::sync::mpsc;
use tokio::task::coop;

use super::error::ErrorKind;

pub use tungstenite::Message;
pub use tungstenite::protocol::frame::{CloseFrame, Utf8Bytes};

pub(super) type Sender = mpsc::Sender<Message>;
pub(super) type Receiver = mpsc::Receiver<Message>;

pub struct Channel(Sender, Receiver);

impl Channel {
    pub(super) fn new() -> (Self, (Sender, Receiver)) {
        let (sender, rx) = mpsc::channel(1);
        let (tx, receiver) = mpsc::channel(1);
        (Self(sender, receiver), (tx, rx))
    }

    pub async fn send(&mut self, message: impl Into<Message>) -> super::Result<()> {
        if self.0.send(message.into()).await.is_err() {
            Err(ControlFlow::Break(ErrorKind::CLOSED.into()))
        } else {
            Ok(())
        }
    }

    pub fn recv(&mut self) -> impl Future<Output = Option<Message>> {
        coop::unconstrained(self.1.recv())
    }
}
