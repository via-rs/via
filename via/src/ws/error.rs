use std::fmt::{self, Display, Formatter};
use std::ops::ControlFlow;

use crate::error::Error;
use crate::guard::header::DenyHeader;

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

#[derive(Debug)]
pub enum UpgradeError {
    SecWebsocketKey,
    SecWebsocketVersion,
    UnknownUpgradeType,
    UpgradeRequired,
    Other,
}

pub fn already_closed() -> ControlFlow<Error, Error> {
    ControlFlow::Break(Error::from_source(Box::new(WebSocketError::AlreadyClosed)))
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

impl std::error::Error for UpgradeError {}

impl Display for UpgradeError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            UpgradeError::UnknownUpgradeType => {
                write!(f, "\"upgrade\" header must contain token: \"websocket\".")
            }
            UpgradeError::UpgradeRequired => {
                write!(f, "\"connection\" header must contain token: \"upgrade\".")
            }
            UpgradeError::SecWebsocketKey => {
                write!(f, "missing required header: \"sec-websocket-key\".")
            }
            UpgradeError::SecWebsocketVersion => {
                write!(f, "\"sec-websocket-version\" must be \"13\".")
            }
            UpgradeError::Other => {
                write!(f, "an unknown error occured during the upgrade handshake.")
            }
        }
    }
}

impl From<DenyHeader<'_>> for UpgradeError {
    fn from(error: DenyHeader<'_>) -> Self {
        match error.name().as_str() {
            "sec-websocket-version" => UpgradeError::SecWebsocketVersion,
            "connection" => UpgradeError::UpgradeRequired,
            "upgrade" => UpgradeError::UnknownUpgradeType,
            _ => UpgradeError::Other,
        }
    }
}

impl ResultExt for Error {
    type Output = ();

    fn or_close(self) -> Result<Self::Output> {
        Err(ControlFlow::Break(self))
    }

    fn or_reconnect(self) -> Result<Self::Output> {
        Err(ControlFlow::Continue(self))
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
