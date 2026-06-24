use crate::Request;

/// A predicate that projects a request's headers before evaluation.
///
/// `Headers<T>` adapts a predicate over [`http::HeaderMap`] so that it may be
/// evaluated directly against a [`Request`].
///
/// This is equivalent to calling [`on`] with the request's headers as the
/// projection target.
///
/// # Example
///
/// ```
/// use via::guard::header::media;
/// use via::guard::{on, header};
///
/// let predicate = on::headers(header::accept(media::json()));
/// ```
pub struct Headers;

/// A predicate that projects a request's method before evaluation.
///
/// `Method<T>` adapts a predicate over [`http::Method`] so that it may be
/// evaluated directly against a [`Request`].
///
/// This is equivalent to calling [`on`] with the request's method as the
/// projection target.
///
/// # Example
///
/// ```
/// use http::Method;
/// use via::guard::method::allow;
/// use via::guard::{self, on};
///
/// let patch_or_put = guard::or((allow(Method::PATCH), allow(Method::PUT)));
/// let predicate = on::method(patch_or_put);;
/// ```
pub struct Method;

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
///     fn project<'a>(&self, request: &'a Request<App>) -> &'a Self::Output {
///         request.uri().path()
///     }
/// }
/// ```
pub trait Project<Input> {
    /// The projected type.
    type Output: ?Sized;

    /// Returns a reference to the projected value.
    fn project<'a>(&self, input: &'a Input) -> &'a Self::Output;
}

impl<App> Project<Request<App>> for Headers {
    type Output = http::HeaderMap;

    #[inline]
    fn project<'a>(&self, request: &'a Request<App>) -> &'a Self::Output {
        request.headers()
    }
}

impl<App> Project<Request<App>> for Method {
    type Output = http::Method;

    #[inline]
    fn project<'a>(&self, request: &'a Request<App>) -> &'a Self::Output {
        request.method()
    }
}
