//! A declarative routing DSL that makes trust boundaries obvious.

mod route;
mod switch;

pub use route::{Prefix, Route};
pub use switch::*;

use std::sync::Arc;
use via_router::{Router as Trie, Traverse};

use crate::middleware::Middleware;

pub struct Router<App> {
    trie: Trie<Arc<dyn Middleware<App>>>,
}

/// Returns a partially applied function that attaches `middleware` to a path.
///
/// This is intended to be used with [`Route::map`], allowing middleware to be
/// composed into a route without having to write a closure.
///
/// # Example
///
/// ```
/// # via::Router::new(|home| {
/// # let mut path = home.prefix();
/// // Define the /api namespace.
/// let mut api = path.push("/api");
///
/// // Start defining descendants of "/api".
/// let mut path = api.prefix();
///
/// // Define the /api/auth resource.
/// path.route("/auth", via::post(login).delete(logout))
///     // Continue defining the /api/auth resource from /api/auth/me.
///     .push("/me")
///     // Requests to /api/auth/me require an authenticated user.
///     .map(via::router::apply(authenticate))
///     // GET /api/auth/me responds with the active user as JSON.
///     .assign(via::get(me));
/// # });
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
    pub fn new(router: impl FnOnce(Route<'_, App>)) -> Self {
        let mut trie = via_router::Router::new();
        let node = trie.route("/");

        router(Route { node });
        Self { trie }
    }

    pub(crate) fn traverse<'b>(&self, path: &'b str) -> Traverse<'_, 'b, Arc<dyn Middleware<App>>> {
        self.trie.traverse(path)
    }
}
