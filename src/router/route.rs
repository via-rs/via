use std::sync::Arc;
use via_router::RouteMut;

use crate::middleware::Middleware;

/// A reborrow of a `&mut Route<App>` that can define a "responder" middleware.
pub struct Index<'a, App>(Route<'a, App>);

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
/// use via::error::{Error, Rescue};
/// use via::{Next, Request, Server};
///
/// #[tokio::main]
/// async fn main() -> Result<ExitCode, Error> {
///     let mut app = via::app(());
///     let mut api = app.route("/api");
///
///     // If an error occurs on a descendant of /api, respond with json.
///     // Siblings of /api must define their own error handling logic.
///     api.uses(Rescue::with(|sanitizer| sanitizer.use_json()));
///
///     // Define a /users resource as a child of /api so the rescue and timeout
///     // middleware run before any of the middleware or responders defined in
///     // the /users resource.
///     api.route("/users").scope(|users| {
///         let index = async |_, _| todo!();
///         let show = async |_, _| todo!();
///
///         // list users
///         users.route("/").to(via::get(index));
///
///         // find user with id = :id
///         users.route("/:id").to(via::get(show));
///     });
///
///     // Start serving our application from http://localhost:8080/.
///     Server::new(app).listen(("127.0.0.1", 8080)).await
/// }
/// ```
pub struct Route<'a, App> {
    pub(super) entry: RouteMut<'a, Arc<dyn Middleware<App>>>,
}

impl<'a, App> Index<'a, App> {
    /// Defines how the route should respond when it is visited.
    ///
    /// Unlike [`Route::to`], the mutable borrow of the route tree entry is
    /// consumed and not returned. This prevents logical aliasing errors that
    /// can occur when defining a tree-like structure with a builder style API.
    pub fn to(self, middleware: impl Middleware<App> + 'static) {
        self.0.to(middleware);
    }
}

impl<'a, App> Route<'a, App> {
    /// Reborrow `self` in order to define a "responder" middleware.
    ///
    /// Route resolution is dependent on the sequence in which they are
    /// defined. In order to encourage clean code and a linear progression of
    /// route definitons, [`Self::to`] takes ownership of `self`. This prevents
    /// descendants from implicitly defining routes on ancestors. [`Self::to`]
    /// returns `self` to support builder-style method chains that allow you to
    /// continue nesting descendant routes from an ancestor.
    ///
    /// Sometimes it can be beneficial to temporarily opt-out of the typical
    /// flow of the router DSL. A common pattern used when defining routes at
    /// scale is moving a route definition at the root of your application's
    /// main fn directly into a scope. This allows common identifiers to be
    /// reused without shadowing identifier names of sibling route definitions.
    ///
    /// # Example
    ///
    /// ```
    /// use via::{Next, Request};
    ///
    /// let mut app = via::app(());
    /// let mut api = app.route("/api");
    ///
    /// api.route("/users").scope(|users| {
    ///     users.uses(async |request: Request, next: Next| {
    ///         // Confirm that the request is authenticated.
    ///         // Then, call the next middleware.
    ///         next.call(request).await
    ///     });
    ///
    ///     let list = via::get(async |request: Request, _: Next| {
    ///         todo!("Respond with a list of users");
    ///     });
    ///
    ///     let show = via::get(async |request: Request, _: Next| {
    ///         todo!("Respond with the user with id = :user-id.");
    ///     });
    ///
    ///     // Reborrow `users` so it can move with `Route::to()`.
    ///     users.index().to(list);
    ///
    ///     // The mutable borrow of `users` is still live.
    ///     users.route("/:user-id").to(show).scope(|user| {
    ///         // Define descendents from /api/users/:user-id.
    ///     });
    /// });
    /// ```
    pub fn index(&mut self) -> Index<'_, App> {
        Index(self.route("/"))
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
    /// // The following routes reference the router entry at /hello/:name.
    /// app.route("/hello/:name");
    /// app.route("/hello").route("/:name");
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
    /// let mut resource = app.route("/posts");
    ///
    /// resource.route("/").to(via::get(posts::index));
    /// resource.route("/:id").to(via::get(posts::show));
    /// resource.route("/trending").to(via::get(posts::trending));
    /// #
    /// # mod posts {
    /// #     use via::{Next, Request};
    /// #     pub async fn trending(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn index(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
    /// # }
    /// ```
    ///
    pub fn route(&mut self, path: &'static str) -> Route<'_, App> {
        Route {
            entry: self.entry.route(path),
        }
    }

    /// Appends the provided middleware to the route's call stack.
    ///
    /// Middleware attached to a route runs anytime the route’s path is a
    /// prefix of the request path.
    ///
    /// # Example
    ///
    /// ```
    /// # use via::{Next, Cookies, Request, raise};
    /// # let mut app = via::app(());
    /// #
    /// // Provides application-wide support for request and response cookies.
    /// app.uses(Cookies::new().allow("is-admin"));
    ///
    /// // Requests made to /admin or any of its descendants must have an
    /// // is-admin cookie present on the request.
    /// app.route("/admin").uses(async |request: Request, next: Next| {
    ///     // We suggest using signed cookies to prevent tampering.
    ///     // See the cookies example in our git repo for more information.
    ///     if request.envelope().cookies().get("is-admin").is_none() {
    ///         raise!(401);
    ///     }
    ///
    ///     next.call(request).await
    /// });
    /// ```
    ///
    pub fn uses(&mut self, middleware: impl Middleware<App> + 'static) {
        self.entry.middleware(Arc::new(middleware));
    }

    /// Consumes self by calling the provided closure with a mutable reference
    /// to self.
    ///
    pub fn scope(mut self, scope: impl FnOnce(&mut Self)) {
        scope(&mut self);
    }

    /// Defines how the route should respond when it is visited.
    ///
    /// # Example
    ///
    /// ```
    /// # mod users {
    /// #     use via::{Next, Request};
    /// #     pub async fn index(_: Request, _: Next) -> via::Result { todo!() }
    /// #     pub async fn show(_: Request, _: Next) -> via::Result { todo!() }
    /// # }
    /// #
    /// # let mut app = via::app(());
    /// #
    /// // Called only when the request path is /users.
    /// let mut users = app.route("/users").to(via::get(users::show));
    ///
    /// // Called only when the request path matches /users/:id.
    /// users.route("/:id").to(via::get(users::show));
    /// ```
    ///
    pub fn to(self, middleware: impl Middleware<App> + 'static) -> Self {
        Self {
            entry: self.entry.to(Arc::new(middleware)),
        }
    }
}
