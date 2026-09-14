use std::convert::Infallible;
use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};
use std::io;
use tokio::task::JoinError;
use tokio::time::error::Elapsed;

use super::BoxError;

#[derive(Debug)]
struct HandshakeTimeoutError;

#[derive(Debug)]
pub(crate) enum ServerError {
    Http(hyper::Error),
    Other(BoxError),
    Join(JoinError),
    ShutdownTimeout,
}

impl Error for HandshakeTimeoutError {}

impl Display for HandshakeTimeoutError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "tls negotiation did not finish within the configured timeout",
        )
    }
}

impl Display for ServerError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Http(error) => Display::fmt(error, f),
            Self::Other(error) => Display::fmt(&**error, f),
            Self::Join(error) => Display::fmt(error, f),
            Self::ShutdownTimeout => write!(f, "shutdown timeout expired"),
        }
    }
}

impl Error for ServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Join(error) => Some(error),
            Self::Other(error) => error.source(),
            Self::ShutdownTimeout => None,
        }
    }
}

impl From<Elapsed> for ServerError {
    fn from(_: Elapsed) -> Self {
        Self::Other(Box::new(HandshakeTimeoutError))
    }
}

impl From<Infallible> for ServerError {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

impl From<hyper::Error> for ServerError {
    fn from(error: hyper::Error) -> Self {
        Self::Http(error)
    }
}

impl From<io::Error> for ServerError {
    fn from(error: io::Error) -> Self {
        Self::Other(Box::new(error))
    }
}

#[cfg(feature = "native-tls")]
impl From<native_tls::Error> for ServerError {
    fn from(error: native_tls::Error) -> Self {
        Self::Other(Box::new(error))
    }
}
