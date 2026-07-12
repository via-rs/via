//! Predicate-based request filters that define the middleware stack shape.
//!
//! Guards are stateless higher-order middleware. They classify a request before
//! asynchronous work begins and decide whether middleware should be entered or
//! skipped entirely.

pub mod bytes;
pub mod error;
pub mod header;
pub mod media;
pub mod method;
pub mod on;

mod predicate;

pub use header::{Header, header};
pub use method::Method;
pub use on::{On, Project, on};
pub use predicate::*;

pub use via_macros::content;

use crate::request::Request;
use crate::{BoxFuture, Continue, Error, Middleware, Next};

/// Skip a middleware if the guard's predicate does not match the request.
///
/// # Example
///
/// ```no_run
/// use std::process::ExitCode;
/// use via::guard::{self, method};
/// use via::{Next, Request, Router, Server};
///
/// async fn cache(request: Request, next: Next) -> via::Result {
///     todo!("implement a simple response cache.");
/// }
///
/// #[tokio::main]
/// async fn main() -> via::Result<ExitCode> {
///     let router = Router::new(|mut home| {
///         // Non-idempotent requests will run the cache middleware.
///         home.middleware(guard::filter(method::is_safe(), cache));
///     });
///
///     Server::new(router, ()).listen(("127.0.0.1", 8080)).await
/// }
/// ```
pub struct Filter<T, U> {
    predicate: T,
    middleware: U,
}

/// Apply a guard's predicate to an individual middleware.
///
/// # Example
///
/// ```no_run
/// mod admin {
///     // Implementations elided...
///     # pub async fn graphql(_: via::Request, _: via::Next) -> via::Result { todo!() }
/// }
///
/// use std::process::ExitCode;
/// use via::guard::{self, on};
/// use via::{Error, Request, Router, Server, err};
///
/// trait Session {
///     fn session(&self) -> Option<&Identity>;
///     fn is_admin(&self) -> bool {
///         self.session().is_some_and(|identity| identity.is_admin)
///     }
/// }
///
/// struct Identity {
///     user_id: u64,
///     is_admin: bool,
/// }
/// #
/// # impl Session for Request {
/// #     fn session(&self) -> Option<&Identity> {
/// #         todo!("implement session restoration and accessors");
/// #     }
/// # }
///
/// #[tokio::main]
/// async fn main() -> Result<ExitCode, Error> {
///     let router = Router::new(|home| {
///         let mut path = home.prefix();
///         let mut api = path.push("/api");
///
///         // Start defining the descendants of "/api".
///         let mut path = api.prefix();
///
///         path.push("/admin/graphql").assign(guard::flat_map(
///             guard::into_error(
///                 |request: &Request| request.is_admin(),
///                 |_| err!(403, "admin permissions are required."),
///             ),
///             via::post(admin::graphql)
///                 .get(admin::graphql)
///                 .or_deny(),
///         ));
///     });
///
///     Server::new(router, ()).listen(("127.0.0.1", 8080)).await
/// }
/// ```
pub struct FlatMap<T, U> {
    predicate: T,
    middleware: U,
}

/// Deny the request if it does not match `predicate`.
///
/// The `guard` fn is preferred when you want every request to a subtree of
/// your app to match `predicate`.
///
/// # Example
///
/// ```rust
/// use via::guard::{self, media};
/// use via::Router;
///
/// let router = Router::new(|home: via::Route| {
///     let mut path = home.prefix();
///     let mut api = path.push("/api");
///
///     // If the client does not speak JSON, deny the request.
///     api.middleware(guard::barrier(guard::content!(media::json())));
///     // Subsequent routes defined from `api` require:
///     //   - accept: application/json [; charset=utf-8], */*
///     //   - content-type: application/json [; charset=utf-8]
///     //   - content-length: ^(\d+)$ <= Server::max_request_size
///
///     // Start defining the descendants of "/api".
///     let mut path = api.prefix();
///
///     path.push("/users").map(|mut users| {
///         // Define the /api/users resource.
///     });
/// });
/// ```
pub fn barrier<T>(predicate: T) -> FlatMap<T, Continue> {
    flat_map(predicate, Continue)
}

/// Call `middleware` if `predicate` matches the request.
///
/// Unlike [`barrier`], the predicate provided only applies to `middleware`.
pub fn filter<T, U>(predicate: T, middleware: U) -> Filter<T, U> {
    Filter {
        predicate,
        middleware,
    }
}

/// Confirm that the request matches `predicate` before calling `middleware`.
///
/// Unlike [`barrier`], the predicate provided only applies to `middleware`.
pub fn flat_map<T, U>(predicate: T, middleware: U) -> FlatMap<T, U> {
    FlatMap {
        predicate,
        middleware,
    }
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
