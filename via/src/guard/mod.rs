pub mod header;
pub mod method;

mod predicate;

pub use header::Header;
pub use predicate::*;

use std::fmt::Debug;

use crate::request::Request;
use crate::{BoxFuture, Continue, Error, Middleware, Next};

/// Content negotation as validation.
pub type Content<T, U> = (
    Header<header::Contains<Or<(header::Media<header::CaseSensitive>, U)>>>,
    When<Not<method::IsSafe>, (Header<T>, Header<Wildcard>)>,
);

/// Skip a middleware if the guard's predicate does not match the request.
///
/// # Example
///
/// ```no_run
/// use std::process::ExitCode;
/// use via::{Request, Next, Server, guard};
///
/// async fn cache(request: Request, next: Next) -> via::Result {
///     todo!("implement a simple response cache.");
/// }
///
/// #[tokio::main]
/// async fn main() -> via::Result<ExitCode> {
///     let mut app = via::app(());
///
///     // Non-idempotent requests will run the cache middleware.
///     app.middleware(guard::filter(guard::method::is_safe(), cache));
///
///     Server::new(app).listen(("127.0.0.1", 8080)).await
/// }
/// ```
pub struct Filter<T, U> {
    predicate: T,
    middleware: U,
}

/// Apply a guard's predicate to an individual middleware.
pub struct FlatMap<T, U> {
    predicate: T,
    middleware: U,
}

/// Call `middleware` if `predicate` matches the request.
pub fn filter<T, U>(predicate: T, middleware: U) -> Filter<T, U> {
    Filter {
        predicate,
        middleware,
    }
}

/// Confirm that the request matches `predicate` before calling `middleware`.
///
/// Unlike [`guard`], the provided predicate only applies to `middleware`.
pub fn flat_map<T, U>(predicate: T, middleware: U) -> FlatMap<T, U> {
    FlatMap {
        predicate,
        middleware,
    }
}

/// Deny the request if it does not match `predicate`.
///
/// The `guard` fn is preferred when you want every request to a subtree of
/// your app to match `predicate`.
///
/// # Example
///
/// ```rust
/// use via::guard::{self, header::media};
///
/// let mut app = via::app(());
/// let mut api = app.route("/api");
///
/// // If the client does not speak JSON, deny the request.
/// api.middleware(guard::content(media::json(), media::json()));
///
/// // Subsequent routes defined from `api` require:
/// //   - Accept: application/json, */*
/// //   - Content-Length: * (<= Server::max_request_size)
/// //   - Content-Type: application/json || application/json; charset=utf-8
///
/// api.route("/users").scope(|users| {
///     // Define the /api/users resource.
/// });
/// ```
pub fn guard<T>(predicate: T) -> FlatMap<T, Continue> {
    flat_map(predicate, Continue)
}

/// Require that the header associated with `key` matches `predicate`.
pub fn header<K, V>(key: K, value: V) -> Header<V>
where
    K: TryInto<http::HeaderName>,
    K::Error: Debug,
{
    Header {
        value,
        key: key.try_into().expect("invalid header name."),
    }
}

/// The client and server agree on a media type and payloads have a known length.
///
/// The first argument is what the client is allowed to send. The second
/// argument is how the server will reply.
pub fn content<T, U>(accepts: T, provides: U) -> FlatMap<Content<T, U>, Continue> {
    guard((
        header::accept(provides),
        when(
            method::is_mutation(),
            (header::content_type(accepts), header::content_length()),
        ),
    ))
}

impl<T, U, App> Middleware<App> for FlatMap<T, U>
where
    T: Predicate<Request<App>> + Send + Sync,
    for<'a> T::Error<'a>: Into<Error>,
    U: Middleware<App>,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        match self.predicate.cmp(&request) {
            Ok(_) => self.middleware.call(request, next),
            Err(error) => {
                let error = error.into();
                Box::pin(async { Err(error) })
            }
        }
    }
}

impl<T, U, App> Middleware<App> for Filter<T, U>
where
    T: Predicate<Request<App>> + Send + Sync,
    U: Middleware<App>,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        if self.predicate.cmp(&request).is_ok() {
            self.middleware.call(request, next)
        } else {
            next.call(request)
        }
    }
}
