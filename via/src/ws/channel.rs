#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::protocol::{CloseFrame, Message, frame::Utf8Bytes};

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
pub use tokio_websockets::{CloseCode, Message};

use futures_channel::mpsc::{self, Receiver, Sender, TryRecvError};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use super::error::{already_closed, readiness_check_required};
use super::util::poll_immediate_no_wake;

pub struct Channel {
    full: Arc<AtomicBool>,
    tx: Sender<Message>,
    rx: Receiver<Message>,
}

struct Send<'a> {
    channel: &'a mut Channel,
    message: Option<Message>,
}

struct Recv<'a> {
    full: &'a AtomicBool,
    recv: mpsc::Recv<'a, mpsc::Receiver<Message>>,
}

#[inline]
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
            channel: self,
            message: Some(message.into()),
        }
    }

    pub fn recv(&mut self) -> impl Future<Output = Option<Message>> {
        Recv {
            full: &*self.full,
            recv: self.rx.recv(),
        }
    }
}

impl Channel {
    pub(super) fn new() -> (Self, Self) {
        let (tx1, rx2) = mpsc::channel(0);
        let (tx2, rx1) = mpsc::channel(0);

        let ours = Self {
            full: Arc::new(AtomicBool::new(false)),
            tx: tx1,
            rx: rx1,
        };

        let theirs = Self {
            full: Arc::clone(&ours.full),
            tx: tx2,
            rx: rx2,
        };

        (ours, theirs)
    }

    pub(super) fn has_outbound(&self) -> bool {
        self.full.load(Ordering::Relaxed)
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
            Ok(outbound) => {
                self.full.store(false, Ordering::Relaxed);
                Ok(Some(outbound))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Closed) => Err(already_closed()),
        }
    }

    /// Try to send the provided message without registering a wake.
    pub(super) fn try_send(&mut self, message: Message) -> super::Result {
        match self.tx.try_send(message) {
            Ok(_) => {
                self.full.store(true, Ordering::Relaxed);
                Ok(())
            }
            Err(error) => {
                if error.is_full() {
                    Err(readiness_check_required())
                } else {
                    Err(already_closed())
                }
            }
        }
    }
}

impl Future for Recv<'_> {
    type Output = Option<Message>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        // Receive progress is driven by the I/O of `WebSocketStream`.
        match Pin::new(&mut self.recv).poll(cx) {
            Poll::Ready(Ok(message)) => {
                self.full.store(false, Ordering::Relaxed);
                Poll::Ready(Some(message))
            }
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(_)) => Poll::Ready(None),
        }
    }
}

impl Send<'_> {
    #[inline]
    fn channel(&mut self) -> &mut Channel {
        &mut self.channel
    }
}

impl Future for Send<'_> {
    type Output = super::Result;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.get_mut(); // reification not required.

        // Send in the body of a receive loop requires back pressure. The
        // following readiness check registers a wake if the channel is full.
        if poll_ready(&mut this.channel.tx, cx)?.is_ready() {
            if let Some(message) = this.message.take() {
                Poll::Ready(this.channel().try_send(message))
            } else {
                log!(warn(ws = 0), "send future polled after completion.");
                Poll::Ready(Ok(()))
            }
        } else {
            // If the sender is blocked because the receiver has not yet
            // received the previous message, return pending. This should be
            // rare.
            //
            // Consider sending a message in a listener like any other write,
            // it implies acquiring a lock and should be done once per listener
            // wake. If you must send more than one message per listener wake,
            // batch them by using a vec in your notification data structure.
            Poll::Pending
        }
    }
}
