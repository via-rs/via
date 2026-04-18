//! Match on a header.

pub mod media;

mod value;

pub use media::Media;
pub use value::*;

use http::header::{self as h, HeaderMap, HeaderName};
use std::fmt::Debug;

use super::{Or, Predicate, Wildcard, header, or, wildcard};
use crate::request::Request;
use crate::{Error, err};

/// Treat the predicate as match if the header is `None`.
pub struct Optional<T>(T);

#[derive(Debug)]
pub enum DenyHeader<'a> {
    Predicate(&'a HeaderName),
    Missing(&'a HeaderName),
}

pub struct Header<T> {
    pub(super) value: T,
    pub(super) key: HeaderName,
}

/// The header associated with `key` must be present in the request.
pub fn exists<K>(key: K) -> Header<Wildcard>
where
    K: TryInto<HeaderName>,
    K::Error: Debug,
{
    header(key, wildcard())
}

/// The value of `Accept` must include `"*/*"` or match `predicate`.
pub fn accept<T>(predicate: T) -> Header<Contains<Or<(Media<CaseSensitive>, T)>>> {
    header(h::ACCEPT, contains(or((media::all(), predicate))))
}

/// The value of `Content-Type` must match `predicate`.
pub fn content_type<T>(predicate: T) -> Header<T> {
    header(h::CONTENT_TYPE, predicate)
}

/// The `Content-Length` header must be present in the request.
pub fn content_length() -> Header<Wildcard> {
    exists(h::CONTENT_LENGTH)
}

impl<T> Header<T> {
    /// If the header associated with  the predicate as match if the header is `None`.
    pub fn optional(self) -> Optional<Self> {
        Optional(self)
    }
}

impl<T> Predicate<HeaderMap> for Header<T>
where
    for<'a> T: Predicate<[u8]> + 'a,
{
    type Error<'a> = DenyHeader<'a>;

    fn cmp<'a>(&'a self, headers: &HeaderMap) -> Result<(), Self::Error<'a>> {
        let key = &self.key;
        let value = headers.get(key).ok_or_else(|| DenyHeader::Missing(key))?;

        self.value
            .cmp(value.as_bytes().trim_ascii())
            .map_err(|_| DenyHeader::Predicate(key))
    }
}

impl<T, App> Predicate<Request<App>> for Header<T>
where
    for<'a> T: Predicate<[u8]> + 'a,
{
    type Error<'a> = DenyHeader<'a>;

    fn cmp<'a>(&'a self, request: &Request<App>) -> Result<(), Self::Error<'a>> {
        self.cmp(request.headers())
    }
}

impl<T, Input> Predicate<Input> for Optional<T>
where
    for<'a> T: Predicate<Input, Error<'a> = DenyHeader<'a>> + 'a,
{
    type Error<'a> = DenyHeader<'a>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.0.cmp(input).or_else(|error| match error {
            DenyHeader::Missing(_) => Ok(()),
            error => Err(error),
        })
    }
}

impl<'a> DenyHeader<'a> {
    pub fn name(&self) -> &'a HeaderName {
        match *self {
            Self::Predicate(name) | Self::Missing(name) => name,
        }
    }
}

impl From<DenyHeader<'_>> for Error {
    fn from(error: DenyHeader<'_>) -> Self {
        match error {
            DenyHeader::Predicate(n) if n == &h::ACCEPT => {
                err!(406, "response media type not supported.")
            }
            DenyHeader::Predicate(n) if n == &h::CONTENT_TYPE => {
                err!(415, "request media type not supported.")
            }
            DenyHeader::Predicate(n) if n == &h::RANGE => {
                err!(416, "unsatisfiable range request.")
            }
            DenyHeader::Predicate(n) if n == &h::UPGRADE => {
                err!(426, "protocol upgrade is not supported.")
            }
            DenyHeader::Predicate(n) | DenyHeader::Missing(n) if n == &h::AUTHORIZATION => {
                err!(401, "unauthorized.")
            }
            DenyHeader::Predicate(n) => {
                err!(400, "invalid value for header: {}.", n)
            }
            DenyHeader::Missing(n) if n == &h::CONTENT_LENGTH => {
                err!(411, "length required.")
            }
            DenyHeader::Missing(n)
                if n == &h::IF_MATCH
                    || n == &h::IF_NONE_MATCH
                    || n == &h::IF_MODIFIED_SINCE
                    || n == &h::IF_UNMODIFIED_SINCE =>
            {
                err!(428, "missing required precondition header: {}.", n)
            }
            DenyHeader::Missing(n) => {
                err!(400, "missing required header: {}.", n)
            }
        }
    }
}
