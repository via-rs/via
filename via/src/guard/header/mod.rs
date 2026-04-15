pub mod media;

mod sequence;
mod tag;

pub use media::Media;
pub use sequence::*;
pub use tag::*;

use http::header::{HeaderMap, HeaderName};
use std::fmt::Debug;

use super::{GuardError, Or, Predicate, or};
use crate::request::Request;

pub type Accept<T> = Contains<Or<(Media, T)>>;

pub struct Optional<T>(T);

#[derive(Debug)]
pub enum HeaderError<'a> {
    Match(&'a HeaderName),
    None(&'a HeaderName),
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

pub fn accept<T>(predicate: T) -> Header<Accept<T>> {
    header(
        http::header::ACCEPT,
        contains(or((media::all(), predicate))),
    )
}

impl<T> Header<T> {
    pub fn optional(self) -> Optional<Self> {
        Optional(self)
    }
}

impl<T> Predicate<HeaderMap> for Header<T>
where
    T: Predicate<[u8]>,
{
    fn cmp<'a>(&'a self, headers: &HeaderMap) -> Result<(), GuardError<'a>> {
        let key = &self.key;

        if let Some(value) = headers.get(key) {
            self.value
                .cmp(value.as_bytes().trim_ascii())
                .map_err(|_| GuardError::Header(HeaderError::Match(key)))
        } else {
            Err(GuardError::Header(HeaderError::None(key)))
        }
    }
}

impl<T, App> Predicate<Request<App>> for Header<T>
where
    T: Predicate<[u8]>,
{
    fn cmp<'a>(&'a self, request: &Request<App>) -> Result<(), GuardError<'a>> {
        self.cmp(request.headers())
    }
}

impl<T, Input> Predicate<Input> for Optional<T>
where
    T: Predicate<Input>,
{
    fn cmp<'a>(&'a self, input: &Input) -> Result<(), GuardError<'a>> {
        self.0.cmp(input).or_else(|error| match error {
            GuardError::Header(HeaderError::None(_)) => Ok(()),
            error => Err(error),
        })
    }
}

impl HeaderError<'_> {
    pub fn name(&self) -> &HeaderName {
        match *self {
            Self::Match(name) | Self::None(name) => name,
        }
    }
}
