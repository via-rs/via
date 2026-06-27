//! Field projection helpers.
//!
//! This module adapts predicates written for one input type so they can be
//! evaluated against another.
//!
//! Most domain predicates in `guard` are implemented for both their natural
//! input type and [`Request`]. For example, header predicates can be evaluated
//! against a [`HeaderMap`] directly, but they can also be evaluated against a
//! [`Request`] for convenience.
//!
//! ```
//! use via::guard::header::{self, media};
//!
//! let accepts_json = header::accept(media::json());
//! ```
//!
//! Projection is useful when more than one predicate should be evaluated
//! against the same projected value. Instead of allowing each predicate to
//! independently access the same request field, [`on`] projects the field once
//! and evaluates the composed predicate against that projected value.
//!
//! ```
//! use via::guard::header::{self, media};
//! use via::guard::on;
//!
//! let content = on::headers((
//!     header::accept(media::json()),
//!     header::content_type(media::json()),
//!     header::content_length(),
//! ));
//! ```
//!
//! The same pattern can be applied to the request method:
//!
//! ```
//! use http::Method;
//! use via::guard::method::allow;
//! use via::guard::{self, on};
//!
//! let is_safe = on::method(guard::or((
//!     allow(Method::GET),
//!     allow(Method::HEAD),
//!     allow(Method::OPTIONS),
//!     allow(Method::TRACE),
//! )));
//! ```
//!
//! Projection keeps predicates reusable without making request evaluation
//! depend on optimizer common-subexpression elimination. Predicates can be
//! written for their smallest natural input, such as [`HeaderMap`] or
//! [`http::Method`], and then lifted to [`Request`] only when needed.
//!
//! Use the helpers in this module, such as [`headers`] and [`method`], when
//! multiple predicates share a well-known request field. Use [`on`] directly
//! for custom projections.
//!
//! [`HeaderMap`]: http::HeaderMap

mod project;

pub use project::*;

use super::Predicate;
use super::error::{OnError, OnErrorKind};

/// Make a predicate's projected input optional.
pub struct Opt<T, U>(On<T, U>);

/// Apply a predicate to a projected field.
///
/// `On<T, U>` transforms the input using a projection and then evaluates
/// the projected value with another predicate.
///
/// This is analogous to calling a method on a structure and testing the
/// returned value.
///
/// Most applications should prefer the helper functions in the [`on`] module
/// rather than constructing `On` directly.
pub struct On<T, U> {
    predicate: T,
    projector: U,
}

/// Project a field before evaluating a predicate.
///
/// The returned predicate first applies `project` to the input and then
/// evaluates `predicate` against the projected value.
///
/// # Example
///
/// ```
/// use via::guard::header::media;
/// use via::guard::on::project;
/// use via::guard::{header, on};
///
/// let predicate = on(project::Headers, header::accept(media::json()));
/// ```
pub fn on<T, U>(predicate: T, projector: U) -> On<T, U> {
    On {
        predicate,
        projector,
    }
}

/// Evaluate a predicate against a request's headers.
///
/// This is a convenience wrapper around [`on`] that projects
/// [`Request::headers`].
///
/// # Example
///
/// ```
/// use via::guard::header::media;
/// use via::guard::{header, on};
///
/// // Requests bodies are JSON and have a known length.
/// let predicate = on::headers((
///     header::content_length(),
///     header::content_type(media::json()),
/// ));
/// ```
pub fn headers<T>(predicate: T) -> On<T, Headers> {
    on(predicate, project::Headers)
}

/// Evaluate a predicate against a request's method.
///
/// This is a convenience wrapper around [`on`] that projects
/// [`Request::method`].
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
pub fn method<T>(predicate: T) -> On<T, Method> {
    on(predicate, Method)
}

pub fn path<T>(predicate: T) -> On<T, Path> {
    on(predicate, Path)
}

pub fn query<T>(predicate: T) -> On<T, Query> {
    on(predicate, Query)
}

pub fn uri<T>(predicate: T) -> On<T, Uri> {
    on(predicate, Uri)
}

impl<T, U> On<T, U> {
    /// Make the predicate's projected input optional.
    ///
    /// If the field projection fails, the predicate returns `Ok(())`.
    pub fn opt(self) -> Opt<T, U> {
        Opt(self)
    }
}

impl<T, U, Input> Predicate<Input> for On<T, U>
where
    for<'a> T: Predicate<U::Output> + 'a,
    for<'a> U: Project<Input> + 'a,
{
    type Error<'a> = OnError<T::Error<'a>, U::Error>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.projector
            .project(input)
            .map_err(OnError::project)
            .and_then(|input| self.predicate.cmp(input).map_err(OnError::predicate))
    }
}

impl<T, U, Input> Predicate<Input> for Opt<T, U>
where
    for<'a> T: Predicate<U::Output> + 'a,
    for<'a> U: Project<Input> + 'a,
{
    type Error<'a> = T::Error<'a>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.0.cmp(input).or_else(|error| match error.kind {
            OnErrorKind::Predicate(error) => Err(error),
            OnErrorKind::Project(_) => Ok(()),
        })
    }
}
