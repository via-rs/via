use http::header::HeaderName;
use std::fmt::Debug;

use super::error::Deny;
use super::predicate::{Opt, Predicate};
use crate::request::Envelope;

pub struct Header<T> {
    value: T,
    key: HeaderName,
}

pub fn all_media() -> impl Predicate<[u8]> {
    super::starts_with(b"*/*")
}

pub fn application_json() -> impl Predicate<[u8]> {
    super::starts_with(b"application/json")
}

pub fn application_json_utf8() -> impl Predicate<[u8]> {
    super::eq(b"application/json; charset=utf-8")
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

impl<T> Predicate<Envelope> for Header<T>
where
    T: Predicate<[u8]>,
{
    fn matches(&self, envelope: &Envelope) -> Result<(), Deny> {
        if envelope
            .headers()
            .get(&self.key)
            .map(|value| value.as_ref())
            .is_some_and(|bytes| self.value.matches(bytes).is_ok())
        {
            Ok(())
        } else {
            Err(Deny::Header(self.key.clone()))
        }
    }
}

impl<T> Predicate<Envelope> for Opt<Header<T>>
where
    T: Predicate<[u8]>,
{
    fn matches(&self, envelope: &Envelope) -> Result<(), Deny> {
        let expr = envelope
            .headers()
            .get(&self.0.key)
            .map(|value| value.as_ref())
            .is_none_or(|bytes| self.0.value.matches(bytes).is_ok());

        if expr {
            Ok(())
        } else {
            Err(Deny::Header(self.0.key.clone()))
        }
    }
}
