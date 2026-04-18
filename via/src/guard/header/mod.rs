//! Match on a header.

pub mod media;

mod value;

pub use media::Media;
pub use value::*;

use http::header::{self as h, HeaderMap, HeaderName};
use std::fmt::Debug;

use super::{Predicate, Wildcard, header, or, wildcard};
use crate::request::Request;
use crate::{Error, err};

/// Treat the predicate as match if the header is `None`.
pub struct Optional<T>(T);

/// A header predicate does not match the request.
#[derive(Debug)]
pub enum DenyHeader<'a> {
    /// The predicate does not match the header value.
    Predicate(&'a HeaderName),

    /// The header is not present on the request.
    Missing(&'a HeaderName),
}

/// Require that a header associated with a key matches a predicate.
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
pub fn accept<T>(predicate: T) -> Header<Contains<media::AllOr<T>>> {
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
        let Some(value) = headers.get(key) else {
            return Err(DenyHeader::Missing(key));
        };

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
            DenyHeader::Predicate(name) => match name.as_str() {
                "accept" => err!(406, "response media type not supported."),
                "authorization" => err!(401, "unauthorized."),
                "content-type" => err!(415, "request media type not supported."),
                "range" => err!(416, "unsatisfiable range request."),
                "upgrade" => err!(426, "protocol upgrade is not supported."),
                n => err!(400, "invalid value for header: {}.", n),
            },
            DenyHeader::Missing(name) => match name.as_str() {
                "content-length" => err!(411, "length required."),

                if_ @ ("if-match"
                | "if-none-match"
                | "if-modified-since"
                | "if-unmodified-since") => {
                    err!(428, "missing required precondition: {}.", if_)
                }

                n => {
                    err!(400, "invalid value for header: {}.", n)
                }
            },
        }
    }
}
