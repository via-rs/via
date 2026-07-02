use std::fmt::{self, Display, Formatter};
use std::ops::ControlFlow;

use crate::error::{Catch, Error};
use crate::guard::error::InvalidHeader;

#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::error::Error as WebSocketError;

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
pub use tokio_websockets::error::Error as WebSocketError;

pub type Result<T = ()> = std::result::Result<T, Catch>;

#[derive(Debug)]
pub enum UpgradeError {
    SecWebsocketKey,
    SecWebsocketVersion,
    UnknownUpgradeType,
    UpgradeRequired,
    Other,
}

pub fn already_closed() -> Catch {
    ControlFlow::Break(Error::from_source(Box::new(WebSocketError::AlreadyClosed)))
}

pub fn rescue(error: WebSocketError) -> Catch {
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

impl From<InvalidHeader<'_>> for UpgradeError {
    fn from(error: InvalidHeader<'_>) -> Self {
        match error.name().as_str() {
            "sec-websocket-version" => UpgradeError::SecWebsocketVersion,
            "connection" => UpgradeError::UpgradeRequired,
            "upgrade" => UpgradeError::UnknownUpgradeType,
            _ => UpgradeError::Other,
        }
    }
}
