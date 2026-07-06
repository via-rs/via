//! A declarative routing DSL that makes trust boundaries obvious.

mod route;
mod switch;

pub use route::Route;
pub use switch::*;

pub(crate) use switch::MethodNotAllowed;

use std::sync::Arc;
use via_router::Traverse;

use crate::middleware::Middleware;

pub(crate) struct Router<App> {
    tree: via_router::Router<Arc<dyn Middleware<App>>>,
}

/// Returns a partially applied function that attaches `middleware` to a route.
///
/// This is intended to be used with [`Route::map`], allowing middleware to be
/// composed into a route without having to write a closure.
///
/// # Example
///
/// ```
/// // Create a new application.
/// let mut app = via::app(());
///
/// // Define the /api namespace
/// let mut path = app.push("/api");
///
/// // Define the /api/auth namespace.
/// path.route("/auth", via::post(login).delete(logout))
///     //                        ^^^^^         ^^^^^^
///     //
///     // The `login` and `logout` middleware both terminate the
///     // request.
///     //
///     // Therefore, `authenticate` can safely be applied after
///     // them without changing their behavior.
///     .map(via::router::apply(authenticate))
///     // Requests to "/api/auth/me" or any descendant of "/auth"
///     // require an authenticated user.
///     .route("/me", via::get(me));
/// #
/// # async fn authenticate(_: via::Request, _: via::Next) -> via::Result { unimplemented!() }
/// # async fn login(_: via::Request, _: via::Next) -> via::Result { unimplemented!() }
/// # async fn logout(_: via::Request, _: via::Next) -> via::Result { unimplemented!() }
/// # async fn me(_: via::Request, _: via::Next) -> via::Result { unimplemented!() }
/// ```
pub fn apply<T, App>(middleware: T) -> impl for<'a> FnOnce(Route<'a, App>) -> Route<'a, App>
where
    T: Middleware<App> + 'static,
{
    |mut path| {
        path.middleware(middleware);
        path
    }
}

impl<App> Router<App> {
    pub fn new() -> Self {
        Self {
            tree: via_router::Router::new(),
        }
    }

    pub fn middleware<T>(&mut self, middleware: T)
    where
        T: Middleware<App> + 'static,
    {
        self.tree.middleware(Arc::new(middleware))
    }

    pub fn push(&mut self, path: &'static str) -> Route<'_, App> {
        Route {
            node: self.tree.route(path),
        }
    }

    pub fn route<T>(&mut self, path: &'static str, middleware: T) -> Route<'_, App>
    where
        T: Middleware<App> + 'static,
    {
        self.push(path).assign(middleware)
    }

    pub fn traverse<'b>(&self, path: &'b str) -> Traverse<'_, 'b, Arc<dyn Middleware<App>>> {
        self.tree.traverse(path)
    }
}
