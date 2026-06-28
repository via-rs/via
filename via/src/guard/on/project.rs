//! Nameable, built-in projection types.

use std::convert::Infallible;

use http::{HeaderMap, HeaderName};

use crate::Request;
use crate::guard::error::MissingUriQuery;

/// Project the request headers.
pub struct Headers;

/// Project the request method.
pub struct Method;

/// Project the request URI path.
pub struct Path;

/// Project the request URI query str.
pub struct Query;

/// Project the request URI.
pub struct Uri;

/// Projects one input type into another.
///
/// Projection allows a predicate written for a specific type to be reused
/// against a larger structure.
///
/// Most applications will use the helper functions provided by this module
/// such as [`headers`] and [`method`] instead of implementing `Project`
/// directly.
///
/// # Example
///
/// ```
/// use via::guard::on::Project;
/// use via::{Request, guard};
///
/// struct Path;
///
/// impl<App> Project<Request<App>> for Path {
///     type Output = str;
///
///     fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Self::Error> {
///         Ok(request.uri().path())
///     }
/// }
/// ```
///
/// [`headers`]: super::headers
/// [`method`]: super::method
pub trait Project<Input> {
    /// The error type returned when projection fails.
    type Error<'a>
    where
        Self: 'a;

    /// The projected value type.
    type Output: ?Sized;

    /// Returns a reference to the projected value.
    fn project<'a, 'b>(&'a self, input: &'b Input) -> Result<&'b Self::Output, Self::Error<'a>>;
}

pub(crate) struct Header {
    pub(super) name: HeaderName,
}

impl<App> Project<Request<App>> for Headers {
    type Error<'a> = Infallible;
    type Output = http::HeaderMap;

    fn project<'a, 'b>(
        &'a self,
        request: &'b Request<App>,
    ) -> Result<&'b Self::Output, Self::Error<'a>> {
        Ok(request.headers())
    }
}

impl Project<HeaderMap> for Header {
    type Error<'a> = &'a HeaderName;
    type Output = [u8];

    fn project<'a, 'b>(
        &'a self,
        headers: &'b HeaderMap,
    ) -> Result<&'b Self::Output, Self::Error<'a>> {
        headers
            .get(&self.name)
            .map(|value| value.as_bytes())
            .ok_or_else(|| &self.name)
    }
}

impl<App> Project<Request<App>> for Method {
    type Error<'a> = Infallible;
    type Output = http::Method;

    fn project<'a, 'b>(
        &'a self,
        request: &'b Request<App>,
    ) -> Result<&'b Self::Output, Self::Error<'a>> {
        Ok(request.method())
    }
}

impl Project<http::Uri> for Query {
    type Error<'a> = MissingUriQuery;
    type Output = [u8];

    fn project<'a, 'b>(&'a self, uri: &'b http::Uri) -> Result<&'b Self::Output, Self::Error<'a>> {
        uri.query().map(str::as_bytes).ok_or(MissingUriQuery)
    }
}

impl<App> Project<Request<App>> for Query {
    type Error<'a> = MissingUriQuery;
    type Output = [u8];

    fn project<'a, 'b>(
        &'a self,
        request: &'b Request<App>,
    ) -> Result<&'b Self::Output, Self::Error<'a>> {
        <Self as Project<http::Uri>>::project(self, request.uri())
    }
}

impl Project<http::Uri> for Path {
    type Error<'a> = Infallible;
    type Output = [u8];

    fn project<'a, 'b>(&'a self, uri: &'b http::Uri) -> Result<&'b Self::Output, Self::Error<'a>> {
        Ok(uri.path().as_bytes())
    }
}

impl<App> Project<Request<App>> for Path {
    type Error<'a> = Infallible;
    type Output = [u8];

    fn project<'a, 'b>(
        &'a self,
        request: &'b Request<App>,
    ) -> Result<&'b Self::Output, Self::Error<'a>> {
        <Self as Project<http::Uri>>::project(self, request.uri())
    }
}

impl<App> Project<Request<App>> for Uri {
    type Error<'a> = Infallible;
    type Output = http::Uri;

    fn project<'a, 'b>(
        &'a self,
        request: &'b Request<App>,
    ) -> Result<&'b Self::Output, Self::Error<'b>> {
        Ok(request.uri())
    }
}
