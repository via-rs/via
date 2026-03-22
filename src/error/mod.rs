//! Error handling.
//!

mod raise;
mod rescue;
mod server;

use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use serde::{Serialize, Serializer};
use std::fmt::{self, Debug, Display, Formatter};
use std::io::{self, Error as IoError};

#[doc(hidden)]
pub use http::StatusCode; // Required for the raise macro.

pub use rescue::{Rescue, Sanitizer};
pub(crate) use server::ServerError;

use crate::response::Response;
use crate::router::MethodNotAllowed;

/// A type alias for `Box<dyn Error + Send + Sync>`.
///
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// An error type that can act as a specialized version of a
/// [`ResponseBuilder`](crate::response::ResponseBuilder).
///
#[derive(Debug)]
pub struct Error {
    status: StatusCode,
    kind: ErrorKind,
}

#[derive(Debug)]
enum ErrorKind {
    AllowMethod(Box<MethodNotAllowed>),
    Message(String),
    Other(BoxError),
    Json(serde_json::Error),
}

enum ErrorKindRef<'a> {
    AllowMethod(&'a MethodNotAllowed),
    Message(&'a str),
    Other(&'a (dyn std::error::Error + 'static)),
    Json(&'a serde_json::Error),
}

#[derive(Serialize)]
#[serde(untagged)]
enum ErrorList<'a> {
    Original(&'a str),
    Chain(Vec<String>),
}

#[derive(Serialize)]
struct Errors<'a> {
    #[serde(serialize_with = "serialize_status_code")]
    status: StatusCode,
    errors: ErrorList<'a>,
}

fn serialize_status_code<S>(status: &StatusCode, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_u16(status.as_u16())
}

impl Error {
    /// Returns a new error with the provided status and message.
    ///
    pub fn new(message: impl Into<String>) -> Self {
        Self::with_status(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    /// Returns a new error with the provided status and message.
    ///
    pub fn with_status(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            kind: ErrorKind::Message(message.into()),
        }
    }

    /// Returns a new error with the provided status and source.
    ///
    pub fn from_source(status: StatusCode, source: BoxError) -> Self {
        Self {
            status,
            kind: ErrorKind::Other(source),
        }
    }

    /// Returns a new error with the provided source a status code derived from
    /// the [`ErrorKind`](io::ErrorKind).
    ///
    pub fn from_io_error(error: IoError) -> Self {
        let status = match error.kind() {
            // Implies a resource already exists.
            io::ErrorKind::AlreadyExists => StatusCode::CONFLICT,

            // Signals a broken connection.
            io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted => StatusCode::BAD_GATEWAY,

            // Suggests the service is not ready or available.
            io::ErrorKind::ConnectionRefused => StatusCode::SERVICE_UNAVAILABLE,

            // Generally indicates a malformed request.
            io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,

            // Implies restricted access.
            io::ErrorKind::IsADirectory
            | io::ErrorKind::NotADirectory
            | io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,

            // Indicates a missing resource.
            io::ErrorKind::NotFound => StatusCode::NOT_FOUND,

            // Implies an upstream service timeout.
            io::ErrorKind::TimedOut => StatusCode::GATEWAY_TIMEOUT,

            // Any other kind is treated as an internal server error.
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        Self::from_source(status, Box::new(error))
    }

    /// Returns a reference to the error source.
    ///
    pub fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind() {
            ErrorKindRef::AllowMethod(source) => Some(source),
            ErrorKindRef::Other(source) => Some(source),
            ErrorKindRef::Json(json) => Some(json),
            _ => None,
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub(crate) fn status_mut(&mut self) -> &mut StatusCode {
        &mut self.status
    }

    fn repr_json(&self, status: StatusCode) -> Errors<'_> {
        if let ErrorKindRef::Message(message) = self.kind() {
            Errors::new(status, message)
        } else {
            let mut errors = Vec::with_capacity(18);
            let mut source = self.source();

            while let Some(error) = source {
                errors.push(error.to_string());
                source = error.source();
            }

            // Reverse the order of the error messages to match the call stack.
            errors.reverse();

            Errors::chain(status, errors)
        }
    }
}

impl Error {
    pub(crate) fn invalid_utf8_sequence(name: &str) -> Self {
        Self::with_status(
            StatusCode::BAD_REQUEST,
            format!("invalid utf-8 sequence of bytes in {}.", name),
        )
    }

    pub(crate) fn method_not_allowed(error: MethodNotAllowed) -> Self {
        Self {
            status: StatusCode::METHOD_NOT_ALLOWED,
            kind: ErrorKind::AllowMethod(Box::new(error)),
        }
    }

    pub(crate) fn require_path_param(name: &str) -> Self {
        Self::with_status(
            StatusCode::BAD_REQUEST,
            format!("missing required path parameter: \"{}\".", name),
        )
    }

    pub(crate) fn require_query_param(name: &str) -> Self {
        Self::with_status(
            StatusCode::BAD_REQUEST,
            format!("missing required query parameter: \"{}\".", name),
        )
    }

    pub(crate) fn ser_json(source: serde_json::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            kind: ErrorKind::Json(source),
        }
    }

    pub(crate) fn de_json(source: serde_json::Error) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            kind: ErrorKind::Json(source),
        }
    }

    #[inline]
    fn kind(&self) -> ErrorKindRef<'_> {
        match &self.kind {
            ErrorKind::AllowMethod(source) => ErrorKindRef::AllowMethod(source),
            ErrorKind::Message(message) => ErrorKindRef::Message(message),
            ErrorKind::Other(source) => ErrorKindRef::Other(source.as_ref()),
            ErrorKind::Json(source) => ErrorKindRef::Json(source),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.kind() {
            ErrorKindRef::Message(message) => Display::fmt(message, f),
            ErrorKindRef::AllowMethod(source) => Display::fmt(source, f),
            ErrorKindRef::Other(source) => Display::fmt(source, f),
            ErrorKindRef::Json(source) => Display::fmt(source, f),
        }
    }
}

impl<E> From<E> for Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(source: E) -> Self {
        Self::from_source(StatusCode::INTERNAL_SERVER_ERROR, Box::new(source))
    }
}

impl From<Error> for Response {
    fn from(error: Error) -> Self {
        let message = error.to_string();
        let content_len = message.len().into();

        let mut response = Self::new(message.into());
        *response.status_mut() = error.status;

        let headers = response.headers_mut();

        headers.insert(CONTENT_LENGTH, content_len);
        if let Ok(content_type) = "text/plain; charset=utf-8".try_into() {
            headers.insert(CONTENT_TYPE, content_type);
        }

        response
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.repr_json(self.status).serialize(serializer)
    }
}

impl<'a> Errors<'a> {
    fn new(status: StatusCode, message: &'a str) -> Self {
        Self {
            status,
            errors: ErrorList::Original(message),
        }
    }

    fn chain(status: StatusCode, chain: Vec<String>) -> Self {
        Self {
            status,
            errors: ErrorList::Chain(chain),
        }
    }
}
