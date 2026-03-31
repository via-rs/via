use crate::{BoxFuture, Middleware, Next, Request};

pub trait Predicate<App> {
    fn matches(&self, request: &Request<App>) -> bool;
}

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
/// use via::{Request, guard, raise};
///
/// let mut app = via::app(());
/// let mut api = app.route("/api");
///
/// api.middleware(via::guard(
///     || raise!(401, message = "\"x-api-key\" is missing or invalid."),
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
/// use via::{Request, raise};
///
/// let mut app = via::app(());
/// let mut api = app.route("/api");
///
/// api.middleware(via::guard(
///     || raise!(401, message = "\"x-api-key\" is missing or invalid."),
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
    E: Fn() -> crate::Result + Send + Sync,
    T: Predicate<App> + Send + Sync,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        if self.predicate.matches(&request) {
            next.call(request)
        } else {
            let result = (self.or_else)();
            Box::pin(async { result })
        }
    }
}

impl<T, App> Predicate<App> for T
where
    T: Fn(&Request<App>) -> bool + Send + Sync,
{
    #[inline]
    fn matches(&self, request: &Request<App>) -> bool {
        (self)(request)
    }
}
