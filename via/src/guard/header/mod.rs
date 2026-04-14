mod sequence;
mod tag;

pub mod accept;
pub mod content_type;

pub use accept::accept;
pub use content_type::content_type;
pub use sequence::*;
pub use tag::*;

use http::{HeaderMap, header::HeaderName};
use std::fmt::Debug;

use super::{ErrorKind, Predicate};
use crate::Request;

pub struct Header<T> {
    optional: bool,
    value: T,
    key: HeaderName,
}

pub fn header<K, V>(key: K, value: V) -> Header<V>
where
    K: TryInto<HeaderName>,
    K::Error: Debug,
{
    Header {
        optional: false,
        value,
        key: key.try_into().expect("invalid header name."),
    }
}

impl<T> Header<T> {
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

impl<T> Predicate<HeaderMap> for Header<T>
where
    T: Predicate<[u8]>,
{
    fn cmp(&self, headers: &HeaderMap) -> Result<(), ErrorKind> {
        match headers.get(&self.key) {
            Some(value) if self.value.cmp(value.as_bytes()).is_ok() => Ok(()),
            None if self.optional => Ok(()),
            _ => Err(ErrorKind::Header(self.key.clone())),
        }
    }
}

impl<T, App> Predicate<Request<App>> for Header<T>
where
    T: Predicate<[u8]>,
{
    fn cmp(&self, request: &Request<App>) -> Result<(), ErrorKind> {
        match request.headers().get(&self.key) {
            Some(value) if self.value.cmp(value.as_bytes()).is_ok() => Ok(()),
            None if self.optional => Ok(()),
            _ => Err(ErrorKind::Header(self.key.clone())),
        }
    }
}
