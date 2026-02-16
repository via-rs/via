use std::fmt::{self, Display, Formatter};
use std::ops::ControlFlow::{Break, Continue};

use crate::error::Error;

#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::error::Error as WebSocketError;

#[cfg(feature = "tokio-websockets")]
pub use tokio_websockets::error::Error as WebSocketError;

type ControlFlow<T> = std::ops::ControlFlow<T, T>;
pub type Result<T = ()> = std::result::Result<T, ControlFlow<Error>>;

pub trait ResultExt {
    type Output;
    fn or_break(self) -> Result<Self::Output>;
    fn or_continue(self) -> Result<Self::Output>;
}

#[derive(Debug)]
pub enum ErrorKind {
    Listener(Error),
    Socket(WebSocketError),
}

#[inline]
pub fn is_recoverable(error: &WebSocketError) -> bool {
    use std::io::ErrorKind;

    match &error {
        WebSocketError::Io(io) => matches!(io.kind(), ErrorKind::Interrupted | ErrorKind::TimedOut),
        _ => false,
    }
}

impl ErrorKind {
    pub const CLOSED: Self = Self::Socket(WebSocketError::AlreadyClosed);
}

impl std::error::Error for ErrorKind {}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Listener(error) => Display::fmt(error, f),
            Self::Socket(error) => Display::fmt(error, f),
        }
    }
}

impl From<Error> for ErrorKind {
    fn from(error: Error) -> Self {
        Self::Listener(error)
    }
}

impl From<hyper::Error> for ErrorKind {
    fn from(error: hyper::Error) -> Self {
        Self::Listener(error.into())
    }
}

impl From<WebSocketError> for ErrorKind {
    fn from(error: WebSocketError) -> Self {
        Self::Socket(error)
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
