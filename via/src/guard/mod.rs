pub mod header;
pub mod method;

mod predicate;

pub use header::{DenyHeader, header};
pub use predicate::*;

use crate::request::Request;
use crate::{BoxFuture, Error, Middleware, Next};

/// Stop processing the request and respond if the provided precondition fails.
///
/// Guard wraps a synchronous check function `T` that receives a reference to
/// the incoming request and returns a result.
///
/// This is useful for lightweight, synchronous validations that do not require
/// async work—such as authorization checks based on headers, request metadata,
/// or other inexpensive predicates.
///
/// # Example
///
/// ```rust
/// use via::{Request, guard, deny};
///
/// let mut app = via::app(());
/// let mut api = app.route("/api");
///
/// api.middleware(via::guard(
///     || deny!(401, "\"x-api-key\" is missing or invalid."),
///     |request: &Request<_>| {
///         request.headers().get("x-api-key").is_some_and(|value| {
///             todo!("validate api key value");
///         })
///     },
/// ));
///
/// // Subsequent routes have a valid API key.
///
/// api.route("/users").scope(|users| {
///     // Define the /api/users resource.
/// });
/// ```
pub struct Guard<T> {
    predicate: T,
}

/// Confirm that request matches the provided predicate before proceeding.
///
/// # Example
///
/// ```rust
/// use via::{Request, deny};
///
/// let mut app = via::app(());
/// let mut api = app.route("/api");
///
/// api.middleware(via::guard(
///     || deny!(401, "\"x-api-key\" is missing or invalid."),
///     |request: &Request<_>| {
///         request.headers().get("x-api-key").is_some_and(|value| {
///             todo!("validate api key value");
///         })
///     },
/// ));
///
/// // Subsequent routes have a valid API key.
///
/// api.route("/users").scope(|users| {
///     // Define the /api/users resource.
/// });
/// ```
///
pub fn guard<T>(predicate: T) -> Guard<T> {
    Guard { predicate }
}

impl<T, App> Middleware<App> for Guard<T>
where
    T: Predicate<Request<App>> + Send + Sync,
    for<'a> T::Error<'a>: Into<Error>,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        match self.predicate.cmp(&request) {
            Ok(_) => next.call(request),
            Err(error) => {
                let error = error.into();
                Box::pin(async { Err(error) })
            }
        }
    }
}
