//! Error handling.
//!

mod macros;
mod rescue;
mod result;
mod server;

use http::{StatusCode, header};
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::fmt::{self, Debug, Display, Formatter};
use std::io::{self, Error as IoError};

pub use rescue::{Rescue, Sanitizer, rescue};
pub use result::ResultExt;
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
    source: ErrorSource,
}

struct ErrorList<'a>(SmallVec<[Cow<'a, str>; 1]>);

#[derive(Debug)]
enum ErrorSource {
    AllowMethod(Box<MethodNotAllowed>),
    Message(String),
    Other(BoxError),
    Json(serde_json::Error),
}

enum ErrorSourceRef<'a> {
    AllowMethod(&'a MethodNotAllowed),
    Message(&'a str),
    Other(&'a (dyn std::error::Error + 'static)),
    Json(&'a serde_json::Error),
}

#[derive(Serialize)]
struct Errors<'a> {
    #[serde(serialize_with = "serialize_status_code")]
    status: StatusCode,
    errors: ErrorList<'a>,
}

pub fn deny<S, M>(status: S, message: M) -> Error
where
    S: TryInto<StatusCode>,
    S::Error: std::error::Error + Send + Sync + 'static,
    M: Into<String>,
{
    match status.try_into() {
        Err(error) => Error::from_source(Box::new(error)),
        Ok(status) => {
            let source = ErrorSource::Message(message.into());
            Error { status, source }
        }
    }
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
    pub fn new(message: String) -> Self {
        Self::new_with_status(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn new_with_status(status: StatusCode, message: String) -> Self {
        Self {
            status,
            source: ErrorSource::Message(message),
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

        Self::from_source_with_status(status, Box::new(error))
    }

    pub fn from_source(source: BoxError) -> Self {
        Self::from_source_with_status(StatusCode::INTERNAL_SERVER_ERROR, source)
    }

    pub fn from_source_with_status(status: StatusCode, source: BoxError) -> Self {
        Self {
            status,
            source: ErrorSource::Other(source),
        }
    }

    /// Returns a reference to the error source.
    ///
    pub fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.as_source() {
            ErrorSourceRef::AllowMethod(source) => Some(source),
            ErrorSourceRef::Other(source) => Some(source),
            ErrorSourceRef::Json(source) => Some(source),
            _ => None,
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }
}

impl Error {
    pub(crate) fn invalid_utf8_sequence(name: &str) -> Self {
        let mut error = Self::new(format!("invalid utf-8 sequence of bytes in {}.", name));
        error.status = StatusCode::BAD_REQUEST;
        error
    }

    pub(crate) fn method_not_allowed(error: MethodNotAllowed) -> Self {
        Self {
            source: ErrorSource::AllowMethod(Box::new(error)),
            status: StatusCode::METHOD_NOT_ALLOWED,
        }
    }

    pub(crate) fn payload_too_large() -> Self {
        let message = "request body exceeds the maximum length".to_owned();

        Self {
            source: ErrorSource::Message(message),
            status: StatusCode::PAYLOAD_TOO_LARGE,
        }
    }

    pub(crate) fn require_path_param(name: &str) -> Self {
        let mut error = Self::new(format!("missing required path parameter: \"{}\".", name));
        error.status = StatusCode::BAD_REQUEST;
        error
    }

    pub(crate) fn require_query_param(name: &str) -> Self {
        let mut error = Self::new(format!("missing required query parameter: \"{}\".", name));
        error.status = StatusCode::BAD_REQUEST;
        error
    }

    pub(crate) fn ser_json(source: serde_json::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            source: ErrorSource::Json(source),
        }
    }

    pub(crate) fn de_json(source: serde_json::Error) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            source: ErrorSource::Json(source),
        }
    }

    #[inline]
    fn as_source(&self) -> ErrorSourceRef<'_> {
        match &self.source {
            ErrorSource::AllowMethod(source) => ErrorSourceRef::AllowMethod(source),
            ErrorSource::Message(message) => ErrorSourceRef::Message(message),
            ErrorSource::Other(source) => ErrorSourceRef::Other(source.as_ref()),
            ErrorSource::Json(source) => ErrorSourceRef::Json(source),
        }
    }

    fn as_message_or_source(&self) -> Result<&str, Option<&(dyn std::error::Error + 'static)>> {
        match &self.source {
            ErrorSource::Message(message) => Ok(message.as_ref()),
            ErrorSource::AllowMethod(error) => Err(Some(error)),
            ErrorSource::Other(error) => Err(Some(&**error)),
            ErrorSource::Json(error) => Err(Some(error)),
        }
    }

    fn repr_json(&self) -> Errors<'_> {
        let mut errors = Errors::new(self.status);

        match self.as_message_or_source() {
            Ok(message) => {
                errors.push(Cow::Borrowed(message));
            }
            Err(mut source) => {
                while let Some(error) = source {
                    errors.push(error.to_string().into());
                    source = error.source();
                }

                errors.reverse();
            }
        }

        errors
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.as_source() {
            ErrorSourceRef::Message(message) => Display::fmt(message, f),
            ErrorSourceRef::AllowMethod(source) => Display::fmt(source, f),
            ErrorSourceRef::Other(source) => Display::fmt(source, f),
            ErrorSourceRef::Json(source) => Display::fmt(source, f),
        }
    }
}

impl From<Error> for BoxError {
    fn from(error: Error) -> Self {
        match error.source {
            ErrorSource::AllowMethod(source) => source,
            ErrorSource::Message(string) => string.into(),
            ErrorSource::Other(source) => source,
            ErrorSource::Json(source) => source.into(),
        }
    }
}

impl<E> From<E> for Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(source: E) -> Self {
        Self::from_source(Box::new(source))
    }
}

impl From<Error> for Response {
    fn from(error: Error) -> Self {
        let message = error.to_string();
        let content_len = message.len().into();

        let mut response = Self::new(message.into());
        *response.status_mut() = error.status;

        let headers = response.headers_mut();

        headers.insert(header::CONTENT_LENGTH, content_len);
        if let Ok(content_type) = "text/plain; charset=utf-8".try_into() {
            headers.insert(header::CONTENT_TYPE, content_type);
        }

        response
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.repr_json().serialize(serializer)
    }
}

impl<'a> Errors<'a> {
    fn new(status: StatusCode) -> Self {
        Self {
            status,
            errors: ErrorList(SmallVec::new()),
        }
    }

    fn push(&mut self, message: Cow<'a, str>) {
        self.errors.0.push(message);
    }

    fn reverse(&mut self) {
        self.errors.0.reverse();
    }
}

impl Serialize for ErrorList<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_seq(Some(self.0.len()))?;

        for message in &self.0 {
            state.serialize_element(message.as_ref())?;
        }

        state.end()
    }
}
