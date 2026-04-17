pub mod header;
pub mod method;

mod predicate;

pub use header::{DenyHeader, Header, header};
pub use predicate::*;

use crate::request::Request;
use crate::{BoxFuture, Error, Middleware, Next};

/// Apply a guard's predicate to an individual middleware.
pub struct AndThen<T, U> {
    predicate: T,
    middleware: U,
}

/// Skip a middleware if the guard's predicate does not match the request.
pub struct FlatMap<T, U> {
    predicate: T,
    middleware: U,
}

/// Stop processing the request and respond if the provided predicate fails.
///
/// This is useful for lightweight, synchronous validations that do not require
/// async work—such as authorization checks based on headers, request metadata,
/// or other inexpensive predicates.
///
/// # Example
///
/// ```rust
/// use via::guard::header::media;
/// use via::guard::{guard, header, method, when};
///
/// let mut app = via::app(());
/// let mut api = app.route("/api");
///
/// // Content negotiation. Deny, if the client doesn't speak JSON.
/// api.middleware(guard((
///     header::accept(media::json()),
///     when(method::is_mutation(), header::content_type(media::json())),
/// )));
///
/// // Subsequent routes defined from `api` require:
/// //   - Accept: application/json, */*
/// //   - Content-Type: application/json || application/json; charset=utf-8
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
/// use via::guard::header::media;
/// use via::guard::{guard, header, method, when};
///
/// let mut app = via::app(());
/// let mut api = app.route("/api");
///
/// // Content negotiation. Deny, if the client doesn't speak JSON.
/// api.middleware(guard((
///     header::accept(media::json()),
///     when(method::is_mutation(), header::content_type(media::json())),
/// )));
///
/// // Subsequent routes defined from `api` require:
/// //   - Accept: application/json, */*
/// //   - Content-Type: application/json || application/json; charset=utf-8
///
/// api.route("/users").scope(|users| {
///     // Define the /api/users resource.
/// });
/// ```
///
pub fn guard<T>(predicate: T) -> Guard<T> {
    Guard { predicate }
}

impl<T, U, App> Middleware<App> for AndThen<T, U>
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

impl<T, U, App> Middleware<App> for FlatMap<T, U>
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

impl<T> Guard<T> {
    /// Return a new middleware that is only called if the predicate in self
    /// matches the request.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::process::ExitCode;
    /// use via::guard::header::{contains, header, tag, tag_no_case};
    /// use via::{Request, Next, Server, guard};
    ///
    /// async fn ws_upgrade(request: Request, next: Next) -> via::Result {
    ///     todo!("implement a custom websocket reactor.");
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> via::Result<ExitCode> {
    ///     let mut app = via::app(());
    ///
    ///     app.route("/chat").to({
    ///         let verify_headers = (
    ///             header("sec-websocket-version", tag(b"13")),
    ///             header("connection", contains(tag_no_case(b"upgrade"))),
    ///             header("upgrade", contains(tag_no_case(b"websocket"))),
    ///         );
    ///
    ///         via::get(guard(verify_headers).and_then(ws_upgrade))
    ///     });
    ///
    ///     Server::new(app).listen(("127.0.0.1", 8080)).await
    /// }
    /// ```
    pub fn and_then<U>(self, middleware: U) -> AndThen<T, U> {
        AndThen {
            predicate: self.predicate,
            middleware,
        }
    }

    /// Return a new middleware that is only called if the predicate in self
    /// matches the request.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::process::ExitCode;
    /// use via::guard::{method, guard};
    /// use via::{Request, Next, Server};
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
    ///     app.middleware(guard(method::is_safe()).flat_map(cache));
    ///
    ///     Server::new(app).listen(("127.0.0.1", 8080)).await
    /// }
    /// ```
    pub fn flat_map<U>(self, middleware: U) -> FlatMap<T, U> {
        FlatMap {
            predicate: self.predicate,
            middleware,
        }
    }
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
