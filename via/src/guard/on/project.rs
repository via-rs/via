//! Nameable, built-in projection types.

use std::convert::Infallible;

use crate::Request;
use crate::guard::error::EmptyQuery;

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
pub trait Project<Input> {
    type Error;

    /// The projected value type.
    type Output: ?Sized;

    /// Returns a reference to the projected value.
    fn project<'a>(&self, input: &'a Input) -> Result<&'a Self::Output, Self::Error>;
}

impl<App> Project<Request<App>> for Headers {
    type Error = Infallible;
    type Output = http::HeaderMap;

    #[inline]
    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Self::Error> {
        Ok(request.headers())
    }
}

impl<App> Project<Request<App>> for Method {
    type Error = Infallible;
    type Output = http::Method;

    #[inline]
    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Self::Error> {
        Ok(request.method())
    }
}

impl Project<http::Uri> for Query {
    type Error = EmptyQuery;
    type Output = [u8];

    #[inline]
    fn project<'a>(&self, uri: &'a http::Uri) -> Result<&'a Self::Output, Self::Error> {
        uri.query().map(str::as_bytes).ok_or(EmptyQuery)
    }
}

impl<App> Project<Request<App>> for Query {
    type Error = EmptyQuery;
    type Output = [u8];

    #[inline]
    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Self::Error> {
        self.project(request.uri())
    }
}

impl Project<http::Uri> for Path {
    type Error = Infallible;
    type Output = [u8];

    #[inline]
    fn project<'a>(&self, uri: &'a http::Uri) -> Result<&'a Self::Output, Self::Error> {
        Ok(uri.path().as_bytes())
    }
}

impl<App> Project<Request<App>> for Path {
    type Error = Infallible;
    type Output = [u8];

    #[inline]
    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Self::Error> {
        self.project(request.uri())
    }
}

impl<App> Project<Request<App>> for Uri {
    type Error = Infallible;
    type Output = http::Uri;

    #[inline]
    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Self::Error> {
        Ok(request.uri())
    }
}
