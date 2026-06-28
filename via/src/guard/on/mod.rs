//! Field projection helpers.

mod project;

use http::HeaderName;
pub use project::*;

use super::Predicate;
use super::error::OnError;

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

/// Make a predicate's projected input optional.
pub struct Opt<T, U> {
    predicate: T,
    projector: U,
}

/// Project a field before evaluating a predicate.
///
/// The returned predicate first applies `project` to the input and then
/// evaluates `predicate` against the projected value.
pub fn on<T, U>(predicate: T, projector: U) -> On<T, U> {
    On {
        predicate,
        projector,
    }
}

/// Evaluate a predicate against a request's headers.
pub fn headers<T>(predicate: T) -> On<T, Headers> {
    on(predicate, project::Headers)
}

/// Evaluate a predicate against a request's method.
///
/// # Example
///
/// ```
/// use http::Method;
/// use via::guard::{self, on, method};
///
/// let patch_or_put = guard::or((method(Method::PATCH), method(Method::PUT)));
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

pub(super) fn header<T>(predicate: T, name: HeaderName) -> On<T, Header> {
    on(predicate, Header { name })
}

impl<T, U> On<T, U> {
    /// Make the predicate's projected input optional.
    ///
    /// If the field projection fails, the predicate returns `Ok(())`.
    pub fn opt(self) -> Opt<T, U> {
        Opt {
            predicate: self.predicate,
            projector: self.projector,
        }
    }
}

impl<T, U, Input> Predicate<Input> for On<T, U>
where
    for<'a> T: Predicate<U::Output> + 'a,
    for<'a> U: Project<Input> + 'a,
{
    type Error<'a> = OnError<T::Error<'a>, U::Error<'a>>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.projector
            .project(input)
            .map_err(OnError::Project)
            .and_then(|input| self.predicate.cmp(input).map_err(OnError::Predicate))
    }
}

impl<T, U, Input> Predicate<Input> for Opt<T, U>
where
    for<'a> T: Predicate<U::Output> + 'a,
    for<'a> U: Project<Input> + 'a,
{
    type Error<'a> = T::Error<'a>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.projector
            .project(input)
            .map_or(Ok(()), |input| self.predicate.cmp(input))
    }
}
