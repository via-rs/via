use std::fmt::{self, Display, Formatter};
use std::ops::ControlFlow;

use crate::error::{Catch, Error};
use crate::guard::error::InvalidHeader;

#[cfg(feature = "tokio-tungstenite")]
pub use tungstenite::error::Error as WebSocketError;

#[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
pub use tokio_websockets::error::Error as WebSocketError;

/// Result type returned by WebSocket listeners.
pub type Result<T = ()> = std::result::Result<T, Catch>;

/// Error produced while validating a WebSocket upgrade request.
#[derive(Debug)]
pub enum UpgradeError {
    /// The `Sec-WebSocket-Key` header is missing or invalid.
    SecWebsocketKey,
    /// The `Sec-WebSocket-Version` header is missing or unsupported.
    SecWebsocketVersion,
    /// The `Upgrade` header does not request a WebSocket upgrade.
    UnknownUpgradeType,
    /// The `Connection` header does not contain `upgrade`.
    UpgradeRequired,
    /// An unspecified WebSocket upgrade error occurred.
    Other,
}

/// Return a terminal WebSocket error for a closed connection.
pub fn already_closed() -> Catch {
    ControlFlow::Break(Error::from_source(Box::new(WebSocketError::AlreadyClosed)))
}

/// Classify a WebSocket error as recoverable or terminal.
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
