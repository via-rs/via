use std::convert::Infallible;

use super::error::MapError;
use crate::Error;

/// Coerce a predicate to a boolean expression and negate it.
pub struct Not<T>(T);

/// One of the predicates in self must match the input.
pub struct Or<T>(T);

/// Conditionally execute the second predicate if the first succeeds.
pub struct When<T, U>(T, U);

/// A predicate that succeeds for any input.
pub struct Any;

/// An inexpensive comparison operation that can fail with context.
///
/// A predicate compares itself against some input and returns `Ok(())` when
/// the input matches. If the input does not match, the predicate returns an
/// associated error. Predicates come in two flavors:
///
/// - Boolean predicates return `Result<(), ()>`. These behave like ordinary
///   boolean comparisons, but use `Result` so they compose with predicates
///   that provide richer error information.
///
/// - Contextual predicates return an error that describes why the comparison
///   failed. These errors may borrow from the predicate itself.
///
/// # Contextual Errors
///
/// Contextual predicate errors are deliberately allowed to borrow from `self`.
/// This lets a predicate return known or pre-validated context about the rule
/// that failed without allowing arbitrary user input to flow into an error
/// response.
///
/// For example, a header predicate may return an error containing the
/// [`HeaderName`] associated with the failed predicate. The header name
/// belongs to the predicate, not to the request. The failed request value is
/// examined, but not retained by the error. When that error is converted into
/// an [`Error`], the response is selected from the known set of header rules
/// supported by the application rather than being derived from the raw header
/// value supplied by the client.
///
/// This is especially important for byte-oriented inputs such as HTTP header
/// values. An invalid header value may not be valid UTF-8, and should not be
/// implicitly written into a response body. By tying contextual errors to the
/// lifetime of the predicate, custom predicates can provide useful failure
/// information while keeping untrusted input out of generated error messages.
///
/// If a predicate needs to expose request-derived data in an error, it should
/// do so explicitly by validating, normalizing, and owning that data before it
/// is converted into a response.
///
/// ## Error Lifetimes
///
/// Predicate errors do not cross asynchronous boundaries.
///
/// Guard middleware evaluates predicates synchronously. If a predicate fails,
/// its error is converted into an owned [`Error`] before the guard returns a
/// future. This means a contextual predicate error may borrow from the
/// predicate itself without moving into the future returned by middleware.
///
/// ```
/// # use via::guard::Predicate;
/// #
/// /// A predicate that requires the input string to match `expected`.
/// struct Equals {
///     expected: Box<str>,
/// }
///
/// /// The failed input did not match `expected`.
/// struct NotEqual<'a> {
///     expected: &'a str,
/// }
///
/// impl Predicate<str> for Equals {
///     type Error<'a> = NotEqual<'a>;
///
///     fn cmp<'a>(&'a self, input: &str) -> Result<(), Self::Error<'a>> {
///         let expected = &*self.expected;
///
///         if input == expected {
///             Ok(())
///         } else {
///             Err(NotEqual { expected })
///         }
///     }
/// }
/// ```
///
/// In this example, `input` is examined but never retained by `NotEqual`.
/// The error borrows `expected` from the predicate, so the error can describe
/// the rule that failed without borrowing from the input value.
///
/// In other words, borrowed predicate errors describe why synchronous
/// classification failed; owned framework errors describe the response that
/// should be generated.
///
/// This distinction is intentional. Contextual predicate errors are designed
/// to describe the rule that failed, not the input that caused it to fail.
///
/// Allowing arbitrary input to flow into a predicate's error type would make
/// it easy to accidentally reflect untrusted data into logs, diagnostics, or
/// HTTP responses. For example, an invalid HTTP header value may contain
/// arbitrary bytes that are not valid UTF-8.
///
/// By tying the lifetime of a contextual error to the predicate rather than
/// the input, Via encourages predicates to report known, trusted context about
/// the failed rule. Input-derived data must be explicitly validated,
/// normalized, and owned before it can become part of an error that is later
/// converted into a response.
///
/// This makes the ownership boundary visible in the type system: predicates
/// may inspect untrusted input, but contextual errors describe trusted rules.
///
/// # Input Specialization
///
/// Predicates may be implemented for more than one input type.
///
/// Prefer implementing a predicate for the smallest input that gives the
/// predicate its meaning. For example, a method predicate can be implemented
/// for [`Method`], while a request-level implementation can delegate to the
/// method implementation.
///
/// ```
/// use http::Method;
/// use via::guard::Predicate;
/// use via::{Error, Request, err};
///
/// pub struct Allow {
///     method: Method,
/// }
///
/// pub struct Deny<'a> {
///     allow: &'a Method,
/// }
///
/// pub fn allow(method: Method) -> Allow {
///     Allow { method }
/// }
///
/// impl Predicate<Method> for Allow {
///     type Error<'a> = ();
///
///     fn cmp<'a>(&'a self, input: &Method) -> Result<(), Self::Error<'a>> {
///         if self.method == input {
///             Ok(())
///         } else {
///             Err(())
///         }
///     }
/// }
///
/// impl<App> Predicate<Request<App>> for Allow {
///     type Error<'a> = Deny<'a>;
///
///     fn cmp<'a>(&'a self, request: &Request<App>) -> Result<(), Self::Error<'a>> {
///         self.cmp(request.method()).map_err(|_| Deny {
///             allow: &self.method,
///         })
///     }
/// }
///
/// impl From<Deny<'_>> for Error {
///     fn from(error: Deny<'_>) -> Self {
///         err!(405, "expected request method to be {}", &error.allow)
///     }
/// }
/// ```
///
/// This keeps the predicate reusable with a projected input while still
/// allowing composite input types to provide richer contextual errors.
///
/// To demonstrate this, let's reimplement the [`method::is_safe`] predicate
/// using combinators rather than [the fn](http::Method::is_safe) defined in
/// `impl Method` from the [`http`] crate. The [`on`] combinator works great
/// for this type of projection:
///
/// ```no_run
/// use http::Method;
/// use std::process::ExitCode;
/// use via::guard::method::allow;
/// use via::guard::{self, on};
/// use via::{Error, Request, Server};
///
/// #[tokio::main]
/// async fn main() -> Result<ExitCode, Error> {
///     // Create a new application.
///     let mut app = via::app(());
///
///     // If the request method is safe, cache the response.
///     app.middleware(guard::filter(
///         on::method(guard::or((
///             allow(Method::GET),
///             allow(Method::HEAD),
///             allow(Method::OPTIONS),
///             allow(Method::TRACE),
///         ))),
///         async |request, next| {
///             todo!("implement response caching");
///         },
///     ));
///
///     // Serve the application at http://localhost:8080/.
///     Server::new(app).listen(("127.0.0.1", 8080)).await
/// }
/// ```
///
/// However, avoid adding specialized implementations solely to avoid repeated
/// accessor calls or short-lived borrows. Prefer the implementation that best
/// describes the predicate's input. In optimized builds, ordinary repeated
/// field access and simple projections are generally good candidates for
/// compiler optimization.
///
/// [`Error`]: crate::Error
/// [`HeaderName`]: http::HeaderName
/// [`Method`]: http::Method
/// [`method::is_safe`]: crate::guard::method::is_safe
/// [`on`]: fn@crate::guard::on
pub trait Predicate<Input: ?Sized> {
    /// The error type returned if the predicate fails.
    type Error<'a>
    where
        Self: 'a;

    /// Compares `self` against `input`.
    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>>;
}

/// Conditionally execute either the second or third predicate based on the
/// first.
pub struct IfElse<P, T, E> {
    predicate: P,
    if_true: T,
    or_else: E,
}

/// Map a predicate's error to a different type.
pub struct MapErr<T, F> {
    predicate: T,
    op: F,
}

// Macros adapted for our use case from the nom crate:
// https://github.com/rust-bakery/nom/blob/main/src/branch/mod.rs

macro_rules! and_impls(
    ($first:ident $second:ident $($id: ident)+) => (
        and_impls!(__impl $first $second; $($id)+);
    );
    (__impl $($current:ident)*; $head:ident $($id: ident)+) => (
        impl_and_predicate!($($current)*);
        and_impls!(__impl $($current)* $head; $($id)+);
    );
    (__impl $($current:ident)*; $head:ident) => (
        impl_and_predicate!($($current)*);
        impl_and_predicate!($($current)* $head);
    );
);

macro_rules! or_impls(
    ($first:ident $second:ident $($id: ident)+) => (
        or_impls!(__impl $first $second; $($id)+);
    );
    (__impl $($current:ident)*; $head:ident $($id: ident)+) => (
        impl_or_predicate!($($current)*);
        or_impls!(__impl $($current)* $head; $($id)+);
    );
    (__impl $($current:ident)*; $head:ident) => (
        impl_or_predicate!($($current)*);
        impl_or_predicate!($($current)* $head);
    );
);

macro_rules! impl_and_predicate {
    ($first:ident $($id:ident)+) => {
        impl<Input, $first, $($id),+> Predicate<Input> for ($first, $($id),+)
        where
            Input: ?Sized,
            for<'a> $first: Predicate<Input> + 'a,
            $(for<'a> $id: Predicate<Input, Error<'a> = $first::Error<'a>> + 'a),+
        {
            type Error<'a> = $first::Error<'a>;

            fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
                #[allow(non_snake_case)]
                let ($first, $($id),+) = self;
                $first.cmp(input)
                    $(.and_then(|_| $id.cmp(input)))+
            }
        }
    };
}

macro_rules! impl_or_predicate {
    ($first:ident $($id:ident)+) => {
        impl<Input, $first, $($id),+> Predicate<Input> for Or<($first, $($id),+)>
        where
            Input: ?Sized,
            for<'a> $first: Predicate<Input> + 'a,
            $(for<'a> $id: Predicate<Input, Error<'a> = $first::Error<'a>> + 'a),+
        {
            type Error<'a> = $first::Error<'a>;

            fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
                #[allow(non_snake_case)]
                let ($first, $($id),+) = &self.0;
                $first.cmp(input)
                    $(.or_else(|_| $id.cmp(input)))+
            }
        }
    };
}

/// Returns a predicate that succeeds for any input.
pub fn any() -> Any {
    Any
}

/// Conditionally execute either the second or third predicate based on the
/// first.
pub fn if_else<P, T, E>(predicate: P, if_true: T, or_else: E) -> IfElse<P, T, E> {
    IfElse {
        predicate,
        if_true,
        or_else,
    }
}

/// Map the provided predicate's error to a different type.
///
/// The returned error type must erase the lifetime of the original error. This
/// combinator is particularlly useful when using a closure as a top-level
/// predicate that must return an `Error` type that can be converted to a
/// [`via::Error`](crate::Error).
///
/// # Example
///
/// ```
/// use http::Version;
/// use via::guard::{self, map_err};
/// use via::{err, Request};
///
/// // Create a new application.
/// let mut app = via::app(());
///
/// // Only support request made with HTTP versions >= 1.1.
/// app.middleware(guard::barrier(map_err(
///     |request: &Request| request.version() > Version::HTTP_10,
///     |_| err!(400, "http version not supported"),
/// )));
/// ```
pub fn map_err<T, F>(predicate: T, op: F) -> MapErr<T, F> {
    MapErr { predicate, op }
}

/// Coerce `predicate` to a boolean expression and negate it.
pub fn not<T>(predicate: T) -> Not<T> {
    Not(predicate)
}

/// One of the predicates in `tuple` must match the input.
pub fn or<T>(tuple: T) -> Or<T> {
    Or(tuple)
}

/// Conditionally execute the second predicate if the first succeeds.
pub fn when<T, U>(first: T, second: U) -> When<T, U> {
    When(first, second)
}

// The maximum length of a tuple is 10.
// This limits the best, worst case cyclomatic complexity to 20.

and_impls!(A B C D E F G H I J);
or_impls!(A B C D E F G H I J);

impl<P, T, E, Input> Predicate<Input> for IfElse<P, T, E>
where
    for<'a> P: Predicate<Input> + 'a,
    for<'a> T: Predicate<Input> + 'a,
    for<'a> E: Predicate<Input, Error<'a> = T::Error<'a>> + 'a,
    Input: ?Sized,
{
    type Error<'a> = E::Error<'a>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        if self.predicate.cmp(input).is_ok() {
            self.if_true.cmp(input)
        } else {
            self.or_else.cmp(input)
        }
    }
}

impl<T, F, Input> Predicate<Input> for MapErr<T, F>
where
    for<'a> T: Predicate<Input> + 'a,
    for<'a> F: Fn(T::Error<'_>) -> Error + Copy + 'a,
    Input: ?Sized,
{
    type Error<'a> = MapError<'a, F, T::Error<'a>>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.predicate
            .cmp(input)
            .map_err(|error| MapError::new(&self.op, error))
    }
}

impl<T, Input> Predicate<Input> for Not<T>
where
    for<'a> T: Predicate<Input> + 'a,
    Input: ?Sized,
{
    type Error<'a> = ();

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        if self.0.cmp(input).is_err() {
            Ok(())
        } else {
            Err(())
        }
    }
}

impl<T, U, Input> Predicate<Input> for When<T, U>
where
    for<'a> T: Predicate<Input> + 'a,
    for<'a> U: Predicate<Input> + 'a,
    Input: ?Sized,
{
    type Error<'a> = U::Error<'a>;

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        self.0.cmp(input).map_or(Ok(()), |_| self.1.cmp(input))
    }
}

impl<Input> Predicate<Input> for Any
where
    Input: ?Sized,
{
    type Error<'a> = Infallible;

    fn cmp<'a>(&'a self, _: &Input) -> Result<(), Self::Error<'a>> {
        Ok(())
    }
}

impl<F, Input> Predicate<Input> for F
where
    Input: ?Sized,
    for<'a> F: Fn(&Input) -> bool + Copy + 'a,
{
    type Error<'a> = ();

    fn cmp<'a>(&'a self, input: &Input) -> Result<(), Self::Error<'a>> {
        if (self)(input) { Ok(()) } else { Err(()) }
    }
}
