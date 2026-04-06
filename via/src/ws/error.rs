use std::ops::ControlFlow;

use crate::error::Error;

#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::error::Error as WebSocketError;

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
pub use tokio_websockets::error::Error as WebSocketError;

pub type Result<T = ()> = std::result::Result<T, ControlFlow<Error, Error>>;

pub trait ResultExt {
    type Output;

    fn or_close(self) -> Result<Self::Output>;
    fn or_reconnect(self) -> Result<Self::Output>;
}

pub fn already_closed() -> ControlFlow<Error, Error> {
    ControlFlow::Break(Error::other(Box::new(WebSocketError::AlreadyClosed)))
}

pub fn rescue(error: WebSocketError) -> ControlFlow<Error, Error> {
    use std::io::ErrorKind;

    if let WebSocketError::Io(io) = &error
        && let ErrorKind::Interrupted | ErrorKind::TimedOut = io.kind()
    {
        ControlFlow::Continue(error.into())
    } else {
        ControlFlow::Break(error.into())
    }
}

impl<T, E> ResultExt for std::result::Result<T, E>
where
    Error: From<E>,
{
    type Output = T;

    #[inline]
    fn or_close(self) -> Result<Self::Output> {
        self.map_err(|error| ControlFlow::Break(error.into()))
    }

    #[inline]
    fn or_reconnect(self) -> Result<Self::Output> {
        self.map_err(|error| ControlFlow::Continue(error.into()))
    }
}
