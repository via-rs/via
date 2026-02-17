use std::ops::ControlFlow::{self, Break, Continue};

use crate::error::Error;

#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::error::Error as WebSocketError;

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
pub use tokio_websockets::error::Error as WebSocketError;

pub type Result<T = ()> = std::result::Result<T, ControlFlow<Error, Error>>;

pub trait ResultExt {
    type Output;

    fn or_break(self) -> Result<Self::Output>;
    fn or_continue(self) -> Result<Self::Output>;
}

pub fn try_rescue(error: WebSocketError) -> ControlFlow<Error, Error> {
    use std::io::ErrorKind;

    if let WebSocketError::Io(io) = &error
        && let ErrorKind::Interrupted | ErrorKind::TimedOut = io.kind()
    {
        Continue(error.into())
    } else {
        Break(error.into())
    }
}

impl<T, E> ResultExt for std::result::Result<T, E>
where
    Error: From<E>,
{
    type Output = T;

    #[inline]
    fn or_break(self) -> Result<Self::Output> {
        self.map_err(|error| Break(error.into()))
    }

    #[inline]
    fn or_continue(self) -> Result<Self::Output> {
        self.map_err(|error| Continue(error.into()))
    }
}
