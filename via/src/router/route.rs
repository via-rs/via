use std::sync::Arc;
use via_router::RouteMut;

use crate::middleware::Middleware;

/// A route prefix from which descenants can be defined.
pub struct Prefix<'a, App = ()> {
    node: RouteMut<'a, Arc<dyn Middleware<App>>>,
}

/// A vacant entry in the route tree associated with a path segment pattern.
///
/// Route definitions are composable and inherit middleware from their
/// ancestors. The order in which routes and their middleware are defined
/// determines the sequence of operations that occur when a user visits a given
/// route.
///
/// A well-structured application strategically defines middleware so that
/// shared behavior is expressed by a common path segment prefix and ordered
/// to reflect its execution sequence.
///
/// # Example
///
/// ```no_run
/// use std::process::ExitCode;
/// use via::{Next, Request, Router, Server, rescue};
///
/// #[tokio::main]
/// async fn main() -> via::Result<ExitCode> {
///     let router = Router::new(|mut home| {
///         let mut path = home.prefix();
///         let mut api = path.push("/api");
///
///         // If an error occurs on a descendant of /api, respond with json.
///         // Siblings of /api must define their own error handling logic.
///         api.middleware(rescue::json().build());
///
///         // Start defining descendants of /api.
///         let mut path = api.prefix();
///
///         // Define a /users resource as a child of /api so the rescue and
///         // timeout middleware run before any of the middleware or
///         // responders defined in the /users resource.
///         path.route("/users", via::get(async |_, _| todo!()))
///             .route("/:user-id", via::get(async |_, _| todo!()));
///     });
///
///     // Start serving our application from http://localhost:8080/.
///     Server::new(router, ()).listen(("127.0.0.1", 8080)).await
/// }
/// ```
pub struct Route<'a, App = ()> {
    pub(super) node: RouteMut<'a, Arc<dyn Middleware<App>>>,
}

impl<'a, App> Prefix<'a, App> {
    /// Returns a new child route by appending the provided path to the current
    /// route.
    ///
    /// The path argument can contain multiple segments. The returned route
    /// always represents the final segment of that path.
    ///
    /// # Example
    ///
    /// ```
    /// # via::Router::<()>::new(|home| {
    /// # let mut path = home.prefix();
    /// path.push("/hello/:name");
    /// # });
    /// ```
    ///
    /// # Dynamic Segments
    ///
    /// Routes can include *dynamic* segments that capture portions of the
    /// request path as parameters. These parameters are made available to
    /// middleware at runtime.
    ///
    /// - `:dynamic` — Matches a single path segment. `/users/:id` matches
    ///   `/users/12345` and captures `"12345"` as `id`.
    ///
    /// - `*splat` — Matches zero or more remaining path segments.
    ///   `/static/*asset` matches `/static/logo.png` or `/static/css/main.css`
    ///   and captures the remainder of the path starting from the splat
    ///   pattern as `asset`. `logo.png` and `css/main.css`.
    ///
    /// Dynamic segments match any path segment, so define them after all
    /// static sibling routes to ensure intended routing behavior.
    pub fn push(&mut self, path: &'static str) -> Route<'_, App> {
        Route {
            node: self.node.route(path),
        }
    }

    /// Takes ownership of self and then calls `op` with a mutable ref to self.
    ///
    /// Maping a route is particularly useful when you want to define routes in
    /// a new scope. It is less busy than assigning the path to a variable in a
    /// block.
    ///
    /// # Example
    ///
    /// ```
    /// # via::Router::new(|home| {
    /// #   let mut path = home.prefix();
    /// #
    ///     // The /posts "collection" resource.
    ///     let mut posts = path.route("/posts", via::get(posts::index));
    ///
    ///     // A bespoke /posts/trending route.
    ///     posts.route("/trending", via::get(posts::trending));
    ///
    ///     // The /posts/:id "member" resource.
    ///     posts.route("/:id", via::get(posts::show));
    /// # });
    /// #
    /// # mod posts {
    /// #     use via::{Next, Request};
    /// #     pub async fn trending(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn index(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
    /// # }
    /// ```
    pub fn route<T>(&mut self, path: &'static str, middleware: T) -> Prefix<'_, App>
    where
        T: Middleware<App> + 'static,
    {
        self.node.middleware(Arc::new(middleware));
    }

    /// Takes ownership of self and then calls `op` with a mutable ref to self.
    pub fn map<T>(self, op: impl FnOnce(Self) -> T) -> T {
        op(self)
    }
}

impl<'a, App> Route<'a, App> {
    /// Appends the provided middleware to the route's call stack.
    ///
    /// Middleware attached to a route runs anytime the route’s path is a
    /// prefix of the request path.
    ///
    /// # Example
    ///
    /// ```
    /// # use via::{Next, Request, Route, cookies, deny};
    /// # via::Router::new(|mut home| {
    /// // Provides application-wide support for request and response cookies.
    /// home.middleware(cookies(["is-admin"]));
    ///
    /// // Start defining descendants of "/".
    /// let mut path = home.prefix();
    ///
    /// // The /admin namespace.
    /// let mut admin = path.push("/admin");
    ///
    /// // Requests made to /admin or any of its descendants must have an
    /// // is-admin cookie present on the request.
    /// admin.middleware(async |request: Request, next: Next| {
    ///     // We suggest using signed cookies to prevent tampering.
    ///     // See the cookies example in our git repo for more information.
    ///     if request.cookies().get("is-admin").is_none() {
    ///         deny!(401);
    ///     }
    ///
    ///     next.call(request).await
    /// });
    /// # });
    /// ```
    pub fn middleware<T>(&mut self, middleware: T)
    where
        T: Middleware<App> + 'static,
    {
        self.node.middleware(Arc::new(middleware));
    }

    /// Defines how the route should respond when it is visited.
    ///
    /// # Example
    ///
    /// ```
    /// # use via::router::Route;
    /// #
    /// # via::Router::new(|home| {
    /// # let mut path = home.prefix();
    /// // The /users resource.
    /// let mut users = path.push("/users");
    ///
    /// // Redefine path as the /users prefix.
    /// let mut path = users.assign(via::get(users::index));
    /// //                          ^^^^^^^^^^^^^^^^^^^^^
    /// // Called only when the request path matches /users.
    ///
    /// // The /users/:user-id resource.
    /// let mut user = path.push("/:user-id");
    ///
    /// user.assign(via::get(users::index));
    /// //          ^^^^^^^^^^^^^^^^^^^^^
    /// //   Called only when the request path matches /users/:id.
    /// # });
    /// #
    /// # mod posts {
    /// #     use via::{Next, Request};
    /// #     pub async fn trending(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn index(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
    /// # }
    /// ```
    pub fn assign<T>(self, middleware: T) -> Prefix<'a, App>
    where
        T: Middleware<App> + 'static,
    {
        Prefix {
            node: self.node.assign(Arc::new(middleware)),
        }
    }

    /// Takes ownership of self and then calls `op` with a mutable ref to self.
    pub fn map<T>(self, op: impl FnOnce(Self) -> T) -> T {
        op(self)
    }

    /// Returns a prefix from which descendant routes can be defined.
    pub fn prefix(self) -> Prefix<'a, App> {
        Prefix { node: self.node }
    }
}
