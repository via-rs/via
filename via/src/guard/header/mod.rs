pub mod media;

mod value;

pub use media::Media;
pub use value::*;

use http::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderName};
use std::fmt::Debug;

use super::predicate::Predicate;
use crate::{Error, Request, deny};

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

pub fn accept(media: Media) -> Header<Contains<Media>> {
    header(http::header::ACCEPT, contains(media))
}

pub fn content_type<T>(predicate: T) -> Header<T> {
    header(http::header::CONTENT_TYPE, predicate)
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
    type Error<'a> = HeaderError<'a>;

    fn cmp<'a>(&'a self, headers: &HeaderMap) -> Result<(), Self::Error<'a>> {
        let key = &self.key;
        let Some(value) = headers.get(key) else {
            return Err(HeaderError::None(key));
        };

        self.value
            .cmp(value.as_bytes().trim_ascii())
            .map_err(|_| HeaderError::Match(key))
    }
}

impl<T, App> Predicate<Request<App>> for Header<T>
where
    for<'a> T: Predicate<[u8], Error<'a> = ()> + 'a,
{
    type Error<'a> = HeaderError<'a>;

    fn cmp<'a>(&'a self, request: &Request<App>) -> Result<(), Self::Error<'a>> {
        self.cmp(request.headers())
    }
}

impl<T, Input> Predicate<Input> for Optional<T>
where
    for<'a> T: Predicate<Input, Error<'a> = HeaderError<'a>> + 'a,
{
    type Error<'a> = HeaderError<'a>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.0.cmp(input).or_else(|error| match error {
            HeaderError::None(_) => Ok(()),
            error => Err(error),
        })
    }
}

impl<'a> HeaderError<'a> {
    pub fn name(&self) -> &'a HeaderName {
        match *self {
            Self::Match(name) | Self::None(name) => name,
        }
    }
}

impl From<HeaderError<'_>> for Error {
    fn from(error: HeaderError<'_>) -> Self {
        match error {
            HeaderError::Match(&ACCEPT) => {
                deny!(406, "response format not supported.")
            }
            HeaderError::Match(&CONTENT_TYPE) => {
                deny!(415, "request format not supported.")
            }
            HeaderError::Match(name) => {
                deny!(400, "invalid value for header: {}.", name)
            }
            HeaderError::None(name) => {
                deny!(400, "missing required header: {}.", name)
            }
        }
    }
}
