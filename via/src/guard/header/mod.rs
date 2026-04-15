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

use crate::Request;

use super::{GuardError, Predicate};

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
    fn cmp<'a>(&'a self, headers: &HeaderMap) -> Result<(), GuardError<'a>> {
        match headers.get(&self.key).map(AsRef::as_ref) {
            Some(bytes) if self.value.cmp(bytes).is_ok() => Ok(()),
            None if self.optional => Ok(()),
            _ => Err(GuardError::Header(&self.key)),
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
