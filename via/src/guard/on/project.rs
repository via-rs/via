//! Nameable, built-in projection types.

use std::convert::Infallible;
use std::marker::PhantomData;

use http::{HeaderMap, HeaderName};

use crate::Request;
use crate::guard::error::{MissingUriQuery, UnknownExtension};

/// Project the request extensions.
pub struct Extensions;

/// Project the request headers.
pub struct Headers;

/// Project the request method.
pub struct Method;

/// Project the request URI path.
pub struct Path;

/// Project the request URI query.
pub struct Query;

/// Project the request URI.
pub struct Uri;

/// Projects one input type into another.
///
/// Projection allows a predicate written for a specific type to be reused
/// against a larger structure. It also prevents the lifetime of the input
/// bubbling up to the bounds of a predicate combinator.
///
/// Without concrete projection types, the borrowed input that is passed to a
/// predicate would have to live as long as self. This means that arbitrary
/// unsanitized user input would be able to make it's way into error response
/// or request logs.
///
/// Most applications will use the helper functions provided by this module
/// such as [`headers`] and [`method`] instead of implementing `Project`
/// directly.
///
/// # Example
///
/// ```
/// use std::convert::Infallible;
/// use via::guard::{self, On, Project};
/// use via::Request;
///
/// pub struct Path;
///
/// pub fn project_path<T>(predicate: T) -> On<T, Path> {
///     guard::on(predicate, Path)
/// }
///
/// impl<App> Project<Request<App>> for Path {
///     type Error<'a> = Infallible; // Projections can fail with context.
///     type Output = str;
///
///     fn project<'a, 'b>(
///         &'a self,
///         request: &'b Request<App>,
///     ) -> Result<&'b Self::Output, Self::Error<'a>> {
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

/// Project the request extension of type `T`.
pub struct Extension<T> {
    pub(super) _ty: PhantomData<T>,
}

pub(crate) struct Header {
    pub(super) name: HeaderName,
}

impl<App> Project<Request<App>> for Extensions {
    type Error<'a> = Infallible;
    type Output = http::Extensions;

    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Infallible> {
        Ok(request.extensions())
    }
}

impl<T> Project<http::Extensions> for Extension<T>
where
    T: Send + Sync + 'static,
{
    type Error<'a> = UnknownExtension;
    type Output = T;

    fn project<'a>(&self, ext: &'a http::Extensions) -> Result<&'a Self::Output, UnknownExtension> {
        ext.get().ok_or(UnknownExtension)
    }
}

impl<T, App> Project<Request<App>> for Extension<T>
where
    T: Send + Sync + 'static,
{
    type Error<'a> = UnknownExtension;
    type Output = T;

    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, UnknownExtension> {
        self.project(request.extensions())
    }
}

impl<App> Project<Request<App>> for Headers {
    type Error<'a> = Infallible;
    type Output = http::HeaderMap;

    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Infallible> {
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
        let name = &self.name;

        if let Some(value) = headers.get(name) {
            Ok(value.as_bytes())
        } else {
            Err(name)
        }
    }
}

impl<App> Project<Request<App>> for Method {
    type Error<'a> = Infallible;
    type Output = http::Method;

    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Infallible> {
        Ok(request.method())
    }
}

impl Project<http::Uri> for Query {
    type Error<'a> = MissingUriQuery;
    type Output = [u8];

    fn project<'a>(&self, uri: &'a http::Uri) -> Result<&'a Self::Output, MissingUriQuery> {
        uri.query().map(str::as_bytes).ok_or(MissingUriQuery)
    }
}

impl<App> Project<Request<App>> for Query {
    type Error<'a> = MissingUriQuery;
    type Output = [u8];

    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, MissingUriQuery> {
        self.project(request.uri())
    }
}

impl Project<http::Uri> for Path {
    type Error<'a> = Infallible;
    type Output = [u8];

    fn project<'a>(&self, uri: &'a http::Uri) -> Result<&'a Self::Output, Infallible> {
        Ok(uri.path().as_bytes())
    }
}

impl<App> Project<Request<App>> for Path {
    type Error<'a> = Infallible;
    type Output = [u8];

    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Infallible> {
        self.project(request.uri())
    }
}

impl<App> Project<Request<App>> for Uri {
    type Error<'a> = Infallible;
    type Output = http::Uri;

    fn project<'a>(&self, request: &'a Request<App>) -> Result<&'a Self::Output, Infallible> {
        Ok(request.uri())
    }
}
