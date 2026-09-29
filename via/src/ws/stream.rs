use futures_core::Stream;
use futures_sink::Sink;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(feature = "tokio-tungstenite")]
use tokio_tungstenite::WebSocketStream;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::WebSocketStream;

use super::channel::Message;
use super::error::WebSocketError;
use crate::server::IoStream;

const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<WebSocketStream<IoStream>>();
};

const _: () = {
    const fn assert_unpin<T: Unpin>() {}
    assert_unpin::<WebSocketStream<IoStream>>();
};

pub(super) struct WebSocketStreamMut {
    io: *mut WebSocketStream<IoStream>,
}

impl WebSocketStreamMut {
    #[inline]
    pub(super) unsafe fn new(io: &mut WebSocketStream<IoStream>) -> Self {
        Self { io }
    }
}

impl WebSocketStreamMut {
    #[inline(always)]
    fn project(self: Pin<&mut Self>) -> Pin<&mut WebSocketStream<IoStream>> {
        // Safety:
        //
        // The raw pointer at `self.io` is always valid because:
        //
        // - `Run` never moves or reassigns the value stored at `stream`
        //
        // - `Self` only occurs as a field of `Facade`, `Facade` can only occur
        //   as a field of `Run` and `Run` can only occur as `Pin<Box<Run>>`
        //
        // - `Run` yields to the runtime after dropping `Facade` replacing it
        //   during the next wake
        //
        // Reification occurs as a result of converting `*mut` to `&mut`.
        unsafe { Pin::map_unchecked_mut(self, |this| &mut *this.io) }
    }
}

// Safety:
//
// In order for `Run` to act as a supervisor of `Facade`, `Run` must construct
// `Facade` with a mutable reference to it's `stream` field. This makes the
// `facade` field of `Run` self-referential.
//
// To properly facilitate this behavior, `Run` constructs the `facade` field
// with a `*mut WebSocketStream` in `WebSocketStreamMut`. We know that this
// borrow is always valid because:
//
// - `Facade` can only exist as a field of `Run`
//
// - `Run` can only be constructed with a stable heap address as
//   `Pin<Box<Run>>`
//
// - `Run` never moves or reassigns the value stored in the `stream` field
//
// - `Run` explicitly drops `facade` before `stream` is dropped and never
//   replaces it in the same call stack that sets `facade` to `None`
unsafe impl Send for WebSocketStreamMut {}

impl Drop for WebSocketStreamMut {
    fn drop(&mut self) {
        // Defensive poisoning. Dereferencing a null ptr and a dangling reference
        // to an I/O stream are both undefined behavior. However, dereferencing a
        // null ptr is inherently less risky on modern operating systems.
        //
        // Accessing a value after it has been dropped is impossible to do in
        // Safe Rust and none of the unsafe blocks found in this module allow
        // it to happen.
        //
        // Soundness relies on `WebsocketStreamMut` being dropped before any
        // other reference to `io` exists. This is upheld by yielding before
        // `Facade` is reconstructed.
        self.io = std::ptr::null_mut();
    }
}

impl Sink<Message> for WebSocketStreamMut {
    type Error = WebSocketError;

    fn start_send(self: Pin<&mut Self>, message: Message) -> Result<(), Self::Error> {
        self.project().start_send(message)
    }

    fn poll_ready(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().poll_ready(context)
    }

    fn poll_close(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().poll_close(context)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().poll_flush(context)
    }
}

impl Stream for WebSocketStreamMut {
    type Item = Result<Message, WebSocketError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().poll_next(context)
    }
}
