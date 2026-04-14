use std::fmt::{self, Display, Formatter};
use std::ops::ControlFlow;

use crate::error::Error;
use crate::guard;

use http::header::{CONNECTION, UPGRADE};
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
    InvalidAcceptEncoding,
    InvalidConnectionHeader,
    InvalidUpgradeHeader,
    EncoderError,
    MissingAcceptKey,
    UnknownVersion,
    Unsupported,
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
            UpgradeError::InvalidAcceptEncoding => {
                write!(f, "header \"sec-websocket-key\" must be base64 encoded.")
            }
            UpgradeError::InvalidConnectionHeader => {
                write!(f, "\"connection\" header must contain token: \"upgrade\".")
            }
            UpgradeError::InvalidUpgradeHeader => {
                write!(f, "\"upgrade\" header must contain token: \"websocket\".")
            }
            UpgradeError::EncoderError => {
                write!(f, "failed to encode \"sec-websocket-accept\".")
            }
            UpgradeError::MissingAcceptKey => {
                write!(f, "missing required header: \"sec-websocket-key\".")
            }
            UpgradeError::UnknownVersion => {
                write!(f, "\"sec-websocket-version\" must be \"13\".")
            }
            UpgradeError::Unsupported => {
                write!(f, "connection does not support websocket upgrades.")
            }
        }
    }
}

impl From<guard::ErrorKind> for UpgradeError {
    fn from(error: guard::ErrorKind) -> Self {
        match error {
            guard::ErrorKind::Header(CONNECTION) => UpgradeError::InvalidConnectionHeader,
            guard::ErrorKind::Header(UPGRADE) => UpgradeError::InvalidUpgradeHeader,
            _ => UpgradeError::UnknownVersion,
        }
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
