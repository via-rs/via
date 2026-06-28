//! Predicate combinators for testing request headers.

use http::header::{self as h, HeaderMap, HeaderName};
use std::fmt::Debug;

use super::bytes::{Contains, Trim, contains, trim};
use super::error::{InvalidHeader, OnError};
use super::on::{self, On};
use super::{Any, OkOr, Predicate, any, media, ok_or, or};
use crate::Request;

/// The value predicate of an `Accept` header.
pub type Accept<T> = Contains<Trim<media::AllOr<T>>>;

/// Require that the header for `key` matches `value`.
pub struct Header<T> {
    predicate: On<OkOr<T, HeaderName>, on::Header>,
}

/// When present, require that the header for `key` matches `value`.
pub struct Opt<T> {
    predicate: on::Opt<OkOr<T, HeaderName>, on::Header>,
}

/// The value of `Accept` must include `"*/*"` or match `predicate`.
pub fn accept<T>(predicate: T) -> Header<Accept<T>> {
    header(
        h::ACCEPT,
        contains(trim(or((media::all(), predicate))), b','),
    )
}

/// The value of `Content-Type` must match `predicate`.
pub fn content_type<T>(predicate: T) -> Header<T> {
    header(h::CONTENT_TYPE, predicate)
}

/// The `Content-Length` header must be present in the request.
pub fn content_length() -> Header<Any> {
    exists(h::CONTENT_LENGTH)
}

/// The header associated with `key` must be present in the request.
pub fn exists<K>(key: K) -> Header<Any>
where
    K: TryInto<HeaderName>,
    K::Error: Debug,
{
    header(key, any())
}

/// Require that the header associated with `key` matches `predicate`.
pub fn header<K, V>(key: K, value: V) -> Header<V>
where
    K: TryInto<http::HeaderName>,
    K::Error: Debug,
{
    let key = key.try_into().expect("invalid header name");
    let value = ok_or(value, key.clone());

    Header {
        predicate: on::header(value, key),
    }
}

impl<T> Header<T> {
    /// Make the header associated with `key`, optional.
    pub fn opt(self) -> Opt<T> {
        Opt {
            predicate: self.predicate.opt(),
        }
    }
}

impl<T> Predicate<HeaderMap> for Header<T>
where
    for<'a> T: Predicate<[u8]> + 'a,
{
    type Error<'a> = InvalidHeader<'a>;

    fn cmp<'a>(&'a self, headers: &HeaderMap) -> Result<(), Self::Error<'a>> {
        self.predicate.cmp(headers).map_err(InvalidHeader::new)
    }
}

impl<T, App> Predicate<Request<App>> for Header<T>
where
    for<'a> T: Predicate<[u8]> + 'a,
{
    type Error<'a> = InvalidHeader<'a>;

    fn cmp<'a>(&'a self, request: &Request<App>) -> Result<(), Self::Error<'a>> {
        self.cmp(request.headers())
    }
}

impl<T> Predicate<HeaderMap> for Opt<T>
where
    for<'a> T: Predicate<[u8]> + 'a,
{
    type Error<'a> = InvalidHeader<'a>;

    fn cmp<'a>(&'a self, headers: &HeaderMap) -> Result<(), Self::Error<'a>> {
        self.predicate
            .cmp(headers)
            .map_err(|error| InvalidHeader::new(OnError::Predicate(error)))
    }
}

impl<T, App> Predicate<Request<App>> for Opt<T>
where
    for<'a> T: Predicate<[u8]> + 'a,
{
    type Error<'a> = InvalidHeader<'a>;

    fn cmp<'a>(&'a self, request: &Request<App>) -> Result<(), Self::Error<'a>> {
        self.cmp(request.headers())
    }
}
