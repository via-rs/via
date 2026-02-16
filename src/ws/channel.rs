use std::ops::ControlFlow;
use tokio::sync::mpsc;
use tokio::task::coop;

use super::error::ErrorKind;

#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::Message;

#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::protocol::frame::{CloseFrame, Utf8Bytes};

#[cfg(feature = "tokio-websockets")]
pub use tokio_websockets::{CloseCode, Message};

#[cfg(feature = "tokio-websockets")]
pub use bytestring::ByteString;

type Tx = mpsc::Sender<Message>;
type Rx = mpsc::Receiver<Message>;

pub struct Channel(Tx, Rx);

impl Channel {
    pub(super) fn new() -> (Self, (Tx, Rx)) {
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
