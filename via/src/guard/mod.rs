pub mod header;
pub mod method;

mod error;
mod predicate;

pub use error::ErrorKind;
pub use header::header;
pub use method::{is_mutation, is_safe};
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
pub struct Guard<E, T> {
    or_else: E,
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
pub fn guard<E, T>(or_else: E, predicate: T) -> Guard<E, T> {
    Guard { or_else, predicate }
}

impl<E, T, App> Middleware<App> for Guard<E, T>
where
    E: Fn(ErrorKind) -> Error + Copy + Send + Sync,
    T: Predicate<Request<App>> + Send + Sync,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        match self.predicate.cmp(&request) {
            Ok(_) => next.call(request),
            Err(kind) => {
                let error = (self.or_else)(kind);
                Box::pin(async { Err(error) })
            }
        }
    }
}
