//! Contextual errors and error helper types.

use http::{HeaderName, Method};

use crate::{Error, err};

/// The request URI query is required.
pub struct MissingUriQuery;

/// The request is missing an extension.
pub struct UnknownExtension;

/// The error type returned by the [`into_error`] combinator.
///
/// [`into_error`]: super::into_error
pub struct ErrorThunk<'a, F, E> {
    op: &'a F,
    error: E,
}

/// A request header is either missing or invalid.
pub struct InvalidHeader<'a> {
    source: OnError<&'a HeaderName, &'a HeaderName>,
}

/// A contextual, `405 Method Not Allowed` error.
pub struct MethodNotAllowed<'a> {
    allow: &'a Method,
}

/// An error originating from a field projection or predicate.
#[derive(Debug)]
pub enum OnError<T, U> {
    Predicate(T),
    Project(U),
}

impl<'a, F, E> ErrorThunk<'a, F, E> {
    pub(super) fn new(op: &'a F, error: E) -> Self {
        Self { op, error }
    }
}

impl<F, E> From<ErrorThunk<'_, F, E>> for Error
where
    F: Fn(E) -> Error,
{
    fn from(into: ErrorThunk<'_, F, E>) -> Self {
        (into.op)(into.error)
    }
}

impl<'a> MethodNotAllowed<'a> {
    pub(super) fn new(allow: &'a Method) -> Self {
        Self { allow }
    }
}

impl From<MethodNotAllowed<'_>> for Error {
    fn from(error: MethodNotAllowed<'_>) -> Self {
        err!(405, "expected request method to be {}", &error.allow)
    }
}

impl<T, U> From<OnError<T, U>> for Error
where
    Error: From<T> + From<U>,
{
    fn from(error: OnError<T, U>) -> Self {
        match error {
            OnError::Predicate(error) => error.into(),
            OnError::Project(error) => error.into(),
        }
    }
}

impl From<MissingUriQuery> for Error {
    fn from(_: MissingUriQuery) -> Self {
        err!(400, "request uri query cannot be empty.")
    }
}

impl<'a> InvalidHeader<'a> {
    pub(super) fn new(source: OnError<&'a HeaderName, &'a HeaderName>) -> Self {
        Self { source }
    }

    #[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
    pub(crate) fn name(&self) -> &HeaderName {
        match self.source {
            OnError::Predicate(name) | OnError::Project(name) => name,
        }
    }
}

impl From<InvalidHeader<'_>> for Error {
    fn from(error: InvalidHeader<'_>) -> Self {
        match error.source {
            // The header was present but the predicate failed.
            OnError::Predicate(name) => match name.as_str() {
                "accept" => {
                    err!(406, "response media type not supported.")
                }
                "authorization" => {
                    err!(401, "unauthorized.")
                }
                "content-type" => {
                    err!(415, "request media type not supported.")
                }
                "range" => {
                    err!(416, "unsatisfiable range request.")
                }
                "upgrade" => {
                    err!(426, "protocol upgrade is not supported.")
                }
                name => {
                    err!(400, "invalid header value: {}.", name)
                }
            },
            // The header is required but was not present in the request.
            OnError::Project(project) => match project.as_str() {
                "content-length" => {
                    err!(411, "length required.")
                }
                name @ ("if-match"
                | "if-none-match"
                | "if-modified-since"
                | "if-unmodified-since") => {
                    err!(428, "missing required precondition: {}.", name)
                }
                name => {
                    err!(400, "missing required header: {}.", name)
                }
            },
        }
    }
}

impl From<UnknownExtension> for Error {
    fn from(_: UnknownExtension) -> Self {
        err!(500, "unknown request extension.")
    }
}
