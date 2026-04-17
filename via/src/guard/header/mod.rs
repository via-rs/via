pub mod media;

mod value;

pub use media::Media;
pub use value::*;

use http::header::{self as h, HeaderMap, HeaderName};
use std::fmt::Debug;

use crate::guard::Predicate;
use crate::request::Request;
use crate::{Error, deny};

pub struct Optional<T>(T);

#[derive(Debug)]
pub enum DenyHeader<'a> {
    Predicate(&'a HeaderName),
    Missing(&'a HeaderName),
}

pub struct Header<T> {
    value: T,
    key: HeaderName,
}

pub fn header<K, V>(key: K, value: V) -> Header<V>
where
    K: TryInto<HeaderName>,
    K::Error: Debug,
{
    Header {
        value,
        key: key.try_into().expect("invalid header name."),
    }
}

pub fn accept(media: Media) -> Header<Contains<Media>> {
    header(h::ACCEPT, contains(media))
}

pub fn content_type<T>(predicate: T) -> Header<T> {
    header(h::CONTENT_TYPE, predicate)
}

impl<T> Header<T> {
    pub fn optional(self) -> Optional<Self> {
        Optional(self)
    }
}

impl<T> Predicate<HeaderMap> for Header<T>
where
    for<'a> T: Predicate<[u8], Error<'a> = ()> + 'a,
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
    for<'a> T: Predicate<[u8], Error<'a> = ()> + 'a,
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
                deny!(406, "response media type not supported.")
            }
            DenyHeader::Predicate(n) if n == &h::CONTENT_TYPE => {
                deny!(415, "request media type not supported.")
            }
            DenyHeader::Predicate(n) if n == &h::RANGE => {
                deny!(416, "unsatisfiable range request.")
            }
            DenyHeader::Predicate(n) if n == &h::UPGRADE => {
                deny!(426, "protocol upgrade is not supported.")
            }
            DenyHeader::Predicate(n) | DenyHeader::Missing(n) if n == &h::AUTHORIZATION => {
                deny!(401, "unauthorized.")
            }
            DenyHeader::Predicate(n) => {
                deny!(400, "invalid value for header: {}.", n)
            }
            DenyHeader::Missing(n) if n == &h::CONTENT_LENGTH => {
                deny!(411, "length required.")
            }
            DenyHeader::Missing(n)
                if n == &h::IF_MATCH
                    || n == &h::IF_NONE_MATCH
                    || n == &h::IF_MODIFIED_SINCE
                    || n == &h::IF_UNMODIFIED_SINCE =>
            {
                deny!(428, "missing required precondition header: {}.", n)
            }
            DenyHeader::Missing(n) => {
                deny!(400, "missing required header: {}.", n)
            }
        }
    }
}
