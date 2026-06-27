//! Contextual errors and error helper types.

use http::Method;

use crate::{Error, err};

/// The request URI query is empty or None.
pub struct EmptyQuery;

/// The error type returned by the [`map_err`](super::map_err) predicate.
pub struct MapError<'a, F, E> {
    convert: &'a F,
    error: E,
}

/// A contextual, `405 Method Not Allowed` error.
pub struct MethodNotAllowed<'a> {
    allow: &'a Method,
}

/// An error originating from a field projection or predicate.
#[derive(Debug)]
pub struct OnError<T, U> {
    pub(super) kind: OnErrorKind<T, U>,
}

/// Represents a field projection or predicate that failed.
#[derive(Debug)]
pub(super) enum OnErrorKind<T, U> {
    Predicate(T),
    Project(U),
}

impl<'a, F, E> MapError<'a, F, E> {
    pub(super) fn new(convert: &'a F, error: E) -> Self {
        Self { convert, error }
    }
}

impl<F, E> From<MapError<'_, F, E>> for Error
where
    F: Fn(E) -> Error,
{
    fn from(into: MapError<'_, F, E>) -> Self {
        (into.convert)(into.error)
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

impl<T, U> OnError<T, U> {
    pub(super) fn predicate(error: T) -> Self {
        Self {
            kind: OnErrorKind::Predicate(error),
        }
    }

    pub(super) fn project(error: U) -> Self {
        Self {
            kind: OnErrorKind::Project(error),
        }
    }
}

impl<T, U> From<OnError<T, U>> for Error
where
    Error: From<T> + From<U>,
{
    fn from(error: OnError<T, U>) -> Self {
        match error.kind {
            OnErrorKind::Predicate(error) => error.into(),
            OnErrorKind::Project(error) => error.into(),
        }
    }
}

impl From<EmptyQuery> for Error {
    fn from(_: EmptyQuery) -> Self {
        err!(400, "request uri query cannot be empty.")
    }
}
