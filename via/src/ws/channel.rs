#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::protocol::{CloseFrame, Message, frame::Utf8Bytes};

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
pub use tokio_websockets::{CloseCode, Message};

use futures_channel::mpsc::{self, Receiver, Sender, TryRecvError};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::error::already_closed;
use super::util::poll_immediate_no_wake;

pub struct Channel {
    tx: Sender<Message>,
    rx: Receiver<Message>,
}

struct Send<'a> {
    sender: &'a mut Sender<Message>,
    message: Option<Message>,
}

struct Recv<'a> {
    recv: mpsc::Recv<'a, mpsc::Receiver<Message>>,
}

fn poll_ready(tx: &mut Sender<Message>, cx: &mut Context) -> Poll<super::Result> {
    match tx.poll_ready(cx) {
        Poll::Pending => Poll::Pending,
        Poll::Ready(Ok(_)) => Poll::Ready(Ok(())),
        Poll::Ready(Err(error)) => {
            if error.is_disconnected() {
                Poll::Ready(Err(already_closed()))
            } else {
                Poll::Pending
            }
        }
    }
}

impl Channel {
    pub fn send(&mut self, message: impl Into<Message>) -> impl Future<Output = super::Result> {
        Send {
            sender: &mut self.tx,
            message: Some(message.into()),
        }
    }

    pub fn recv(&mut self) -> impl Future<Output = Option<Message>> {
        Recv {
            recv: self.rx.recv(),
        }
    }
}

impl Channel {
    pub(super) fn new() -> (Self, Self) {
        let (tx1, rx2) = mpsc::channel(0);
        let (tx2, rx1) = mpsc::channel(0);

        (Self { tx: tx1, rx: rx1 }, Self { tx: tx2, rx: rx2 })
    }

    /// Check the capacity of the channel without registering a wake.
    pub(super) fn has_capacity(&mut self) -> super::Result<bool> {
        // Channel progress is driven by the I/O of `WebSocketStream`. Poll
        // without registering a wake.
        match poll_immediate_no_wake(|noop| poll_ready(&mut self.tx, noop)) {
            Poll::Ready(result) => result.and(Ok(true)),
            Poll::Pending => Ok(false),
        }
    }

    /// Try to receive the next message without registering a wake.
    pub(super) fn try_recv(&mut self) -> super::Result<Option<Message>> {
        match self.rx.try_recv() {
            Ok(outbound) => Ok(Some(outbound)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Closed) => Err(already_closed()),
        }
    }

    /// Try to send the provided message without registering a wake.
    pub(super) fn try_send(&mut self, message: Message) -> super::Result {
        self.tx.try_send(message).map_err(|error| {
            debug_assert!(
                error.into_send_error().is_full(),
                "via::ws::Channel::try_send(..) requires a readiness check."
            );

            already_closed()
        })
    }
}

impl Future for Recv<'_> {
    type Output = Option<Message>;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context) -> Poll<Self::Output> {
        // Receive progress is driven by the I/O of `WebSocketStream`.
        poll_immediate_no_wake(|noop| Pin::new(&mut self.recv).poll(noop)).map(Result::ok)
    }
}

impl Future for Send<'_> {
    type Output = super::Result;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        // Sending a message in the body of a receive loop requires back
        // pressure.
        //
        // If the sender is blocked because the receiver has not yet received
        // the previous message, return pending.
        if poll_ready(self.sender, cx)?.is_pending() {
            return Poll::Pending;
        }

        let Some(message) = self.message.take() else {
            // The message was received by the sender.
            return Poll::Ready(Ok(()));
        };

        match self.sender.try_send(message) {
            Ok(_) => Poll::Ready(Ok(())),
            Err(error) => {
                if error.is_disconnected() {
                    Poll::Ready(Err(already_closed()))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}
