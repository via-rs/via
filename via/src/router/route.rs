use std::sync::Arc;
use via_router::RouteMut;

use crate::middleware::Middleware;

/// An entry in the route tree associated with a path segment pattern.
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
/// use via::{Error, Next, Request, Server, rescue};
///
/// #[tokio::main]
/// async fn main() -> Result<ExitCode, Error> {
///     let mut app = via::app(());
///     let mut path = app.push("/api");
///
///     // If an error occurs on a descendant of /api, respond with json.
///     // Siblings of /api must define their own error handling logic.
///     path.middleware(rescue(|sanitizer| sanitizer.use_json()));
///
///     // Define a /users resource as a child of /api so the rescue and timeout
///     // middleware run before any of the middleware or responders defined in
///     // the /users resource.
///     path.route("/users", via::get(async |_, _| todo!()))
///         .route("/:user-id", via::get(async |_, _| todo!()));
///
///     // Start serving our application from http://localhost:8080/.
///     Server::new(app).listen(("127.0.0.1", 8080)).await
/// }
/// ```
pub struct Route<'a, App> {
    pub(super) node: RouteMut<'a, Arc<dyn Middleware<App>>>,
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
    /// # use via::{Next, Request, cookies, deny};
    /// # let mut app = via::app(());
    /// #
    /// // Provides application-wide support for request and response cookies.
    /// app.middleware(cookies(["is-admin"]));
    ///
    /// // Requests made to /admin or any of its descendants must have an
    /// // is-admin cookie present on the request.
    /// app.push("/admin").middleware(async |request: Request, next: Next| {
    ///     // We suggest using signed cookies to prevent tampering.
    ///     // See the cookies example in our git repo for more information.
    ///     if request.cookies().get("is-admin").is_none() {
    ///         deny!(401);
    ///     }
    ///
    ///     next.call(request).await
    /// });
    /// ```
    ///
    pub fn middleware<T>(&mut self, middleware: T)
    where
        T: Middleware<App> + 'static,
    {
        self.node.middleware(Arc::new(middleware));
    }

    /// Returns a new child route by appending the provided path to the current
    /// route.
    ///
    /// The path argument can contain multiple segments. The returned route
    /// always represents the final segment of that path.
    ///
    /// # Example
    ///
    /// ```
    /// # let mut app = via::app(());
    /// // The following routes all reference the same path: /hello/:name.
    /// app.push("/hello/:name");
    /// app.push("hello").push(":name");
    /// app.push("/hello").push("/:name");
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
    ///
    /// Consider the following sequence of route definitions. We define
    /// `/articles/trending` before `/articles/:id` to ensure that a request to
    /// `/articles/trending` is routed to `articles::trending` rather than
    /// capturing `"trending"` as `id` and invoking `articles::show`.
    ///
    /// ```
    /// # let mut app = via::app(());
    /// let mut path = app.push("/posts");
    ///
    /// // Assigning a terminal middleware to a path takes ownership of self.
    /// //
    /// // We want to continue defining adjacent resources rather than
    /// // dependencies of an individual post. Therefore, we reassign path.
    /// path = path.assign(via::get(posts::index));
    ///
    /// // The /posts/:id resource.
    /// path.push("/:id").assign(via::get(posts::show));
    ///
    /// // A bespoke /posts/trending route.
    /// path.push("/trending").assign(via::get(posts::trending));
    /// #
    /// # mod posts {
    /// #     use via::{Next, Request};
    /// #     pub async fn trending(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn index(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
    /// # }
    /// ```
    pub fn push(&mut self, path: &'static str) -> Route<'_, App> {
        Route {
            node: self.node.route(path),
        }
    }

    /// A convenience method that appends `path` to self and assigns it to
    /// `middleware`.
    ///
    /// # Example
    ///
    /// ```
    /// # let mut app = via::app(());
    /// let mut path = app.push("/posts");
    ///
    /// // The /posts "collection" resource.
    /// path = path.assign(via::get(posts::index));
    ///
    /// // The /posts/:id "member" resource.
    /// path.route("/:id", via::get(posts::show));
    ///
    /// // A bespoke /posts/trending route.
    /// path.route("/trending", via::get(posts::trending));
    /// #
    /// # mod posts {
    /// #     use via::{Next, Request};
    /// #     pub async fn trending(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn index(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
    /// # }
    /// ```
    pub fn route<T>(&mut self, path: &'static str, middleware: T) -> Route<'_, App>
    where
        T: Middleware<App> + 'static,
    {
        self.push(path).assign(middleware)
    }

    /// Defines how the route should respond when it is visited.
    ///
    /// # Example
    ///
    /// ```
    /// # let mut app = via::app(());
    /// #
    /// // Define the /users resource as `path`.
    /// let mut path = app.push("/users");
    ///
    /// path.assign(via::get(users::index))
    /// //          ^^^^^^^^^^^^^^^^^^^^^
    /// //   Called only when the request path matches /users.
    /// //
    ///     .push("/:id")
    ///     .assign(via::get(users::show));
    /// //          ^^^^^^^^^^^^^^^^^^^^^
    /// //   Called only when the request path matches /users/:id.
    /// //
    /// #
    /// # mod users {
    /// #     use via::{Next, Request};
    /// #     pub async fn index(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
    /// # }
    /// ```
    pub fn assign<T>(self, middleware: T) -> Self
    where
        T: Middleware<App> + 'static,
    {
        Self {
            node: self.node.assign(Arc::new(middleware)),
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
    /// # let mut app = via::app(());
    /// #
    /// // The /posts "collection" resource.
    /// app.route("/posts", via::get(posts::index)).map(|mut path| {
    ///     // The /posts/:id "member" resource.
    ///     path.route("/:post-id", via::get(posts::show));
    ///
    ///     // A bespoke /posts/trending route.
    ///     path.route("/:post-id", via::get(posts::trending));
    /// });
    /// #
    /// # mod posts {
    /// #     use via::{Next, Request};
    /// #     pub async fn trending(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn index(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
    /// # }
    /// ```
    ///
    pub fn map<T>(self, op: impl FnOnce(Self) -> T) -> T {
        op(self)
    }
}
