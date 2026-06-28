use std::future::Future;
use std::pin::Pin;

use super::next::Next;
use crate::error::Error;
use crate::request::Request;
use crate::response::Response;

/// An alias for the pin
/// [`Box<dyn Future>`]
/// returned by
/// [middleware](Middleware::call).
///
pub type BoxFuture<T = Result> = Pin<Box<dyn Future<Output = T> + Send>>;

/// An alias for results that uses the [`Error`] struct defined in this crate.
///
pub type Result<T = Response> = std::result::Result<T, Error>;

/// An asynchronous step in the request processing pipeline.
///
/// Middleware is executed in the order it appears in the resolved route stack.
/// Each middleware in the resolved route stack receives ownership of `request`
/// and `next`, allowing it to build a response, return an error, or delegate
/// with `next.call(request)`.
///
/// When an implementation of middleware decides not to call `next`, the
/// remaining middleware are not executed. We refer to middleware that ignore
/// the `next` argument entirely as "terminal middleware".
///
/// # Terminal Middleware
///
/// Every non-empty route stack contains at least one terminal middleware.
///
/// In most applications, terminal middleware is where the actual work happens.
/// Middleware may authenticate a request, parse cookies, perform validation,
/// but eventually some terminal middleware will generate a response.
///
/// ```
/// use via::{Next, Request, Response, ResultExt};
///
/// async fn hello(request: Request, _: Next) -> via::Result {
///     //                           ^^^^^^^
///     // The `next` argument is ignored. This is a terminal
///     // middleware.
///     //
///     // Get a reference to `name` from the request uri path.
///     let name = request.param("name").into_result()?;
///
///     // Build a personalized greeting with the name parameter.
///     let greeting = format!("Hello, {}!", name.as_ref());
///
///     // Send a plain text response with our greeting message.
///     Response::build().text(greeting)
/// }
/// ```
///
/// Terminal middleware occurs as the last step of a route stack's request
/// processing pipeline. Therefore, an `async fn` or async closure is just as
/// efficient as manually boxing an async block as you would in an explicit
/// implementation of `Middleware` for a concrete type.
///
/// For this reason, we suggest writing terminal middleware as an `async fn`.
/// They cannot capture state from their environment and the `.await` can be
/// used throughout the entire body of the function. No proc-macro necessary.
///
/// Sometimes, it is beneficial to perform some work before calling the next
/// middleware in the chain. The blanket implementation of `Middleware` for a
/// non-capturing closure (i.e any closure that implements [`Copy`]) supports
/// performing work before a box future is returned. However, calling `next`
/// in an async context always introduces an extra layer of pointer
/// indirection.
///
/// Therefore, we suggest implementing "higher-order middleware" or middleware
/// that occurs before a terminal middleware with the [`middleware`] fn or as
/// an explicit impl for a concrete type if it must be nameable.
///
/// # Higher-Order Middleware
///
/// Higher-order middleware modifies how subsequent middleware behaves or
/// decorates the response built by a terminal middleware.
///
/// Examples of higher-order middleware include:
///
/// - Authentication middleware that decides whether a request can continue
///
/// - Request guards conditionally call middleware based on request metadata
///
/// - Logging middleware that records metadata about the request and response
///
/// - Cookie middleware that decorate the request with parsed cookies and the
///   response with `set-cookie` headers generated from the delta of cookies
///   that change in subsequent middleware
///
/// Higher-order middleware can be either synchronous or asynchronous. If the
/// work performed by a middleware can be done synchronously, you can avoid a
/// layer of pointer indirection by delegating directly to the next middleware
/// in the chain. If a middleware can perform all of its work synchronously, it
/// is likely that it does not transfer ownership of state to the box future
/// returned from [`Middleware::call`]. Therefore, we refer to higher-order
/// middleware that does not introduce an additional layer of indirection as
/// "stateless".
///
/// ## Stateless Middleware
///
/// Some higher-order middleware can perform all of its work synchronously and
/// then delegate directly to the next middleware in the chain. Request guards
/// are a common example. A guard examines the request and either forwards it
/// to another middleware or rejects it immediately. Let's take a look at the
/// implementation of the [`FlatMap`](crate::guard::FlatMap) guard.
///
/// ```
/// use via::{BoxFuture, Error, Middleware, Next, Request};
/// use via::guard::Predicate;
///
/// struct FlatMap<T, U> {
///     predicate: T,
///     middleware: U,
/// }
///
/// impl<T, U, App> Middleware<App> for FlatMap<T, U>
/// where
///     T: Predicate<Request<App>> + Send + Sync,
///     for<'a> T::Error<'a>: Into<Error>,
///     U: Middleware<App>,
/// {
///     fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
///         match self.predicate.cmp(&request) {
///             Ok(_) => self.middleware.call(request, next),
///             Err(error) => {
///                 let error = error.into();
///                 Box::pin(async { Err(error) })
///             }
///         }
///     }
/// }
/// ```
///
/// Notice that when the predicate succeeds, the guard simply returns the
/// future produced by the inner middleware. No additional asynchronous state
/// is needed, so the middleware does not need to wrap the future returned by
/// `next`.
///
/// This makes stateless middleware extremely cheap. Aside from the work
/// performed by the predicate itself, there is little overhead beyond the
/// normal middleware dispatch already required by Via.
///
/// Whenever your middleware can do all of its work synchronously, this style
/// is usually preferred. It avoids an additional layer of pointer indirection.
/// You can further optimize a route stack by composing a stateless middleware
/// with a terminal middleware to collapse the cost of 2 middleware dispatches
/// to one. This means one layer of pointer indirection, one dynamic dispatch
/// call, and single `Arc::clone` for two discrete middlewares.
///
/// ### Guard-Amortized Middleware Execution
///
/// Some middleware compositions allow request processing cost to be amortized
/// across guard boundaries, rather than paid unconditionally for every
/// request.
///
/// In these cases, [guards] can eliminate the cost of downstream middleware by
/// rejecting requests before additional middleware is entered. This allows
/// entire middleware layers to be skipped without allocation or asynchronous
/// suspension.
///
/// Consider the following pattern:
///
/// - A guard evaluates request metadata (headers, method, authentication state)
///
/// - If the guard succeeds, execution continues into a nested middleware stack
///
/// - If the guard fails, the nested middleware is never constructed or polled
///
/// This means the effective cost of middleware is no longer strictly additive
/// across the route stack. Instead, cost becomes path-dependent, and can
/// collapse depending on early guard evaluation.
///
/// ```no_run
/// use std::process::ExitCode;
/// use via::guard::{self, media, method};
/// use via::{Error, Server, cookies, rescue};
///
/// mod session {
///     // Implementations elided...
/// #   use via::{BoxFuture, Next, Middleware, Request, Response};
/// #
/// #   pub const COOKIE: &str = "via-session";
/// #
/// #   /// Returns `true` if the active user session needs verification.
/// #   pub fn is_expired(request: &Request) -> bool { todo!() }
/// #
/// #   /// Extract the active user session from the session cookie.
/// #   pub fn restore() -> impl Middleware<()> { Restore }
/// #
/// #   /// Confirms that the active user exists and has a valid account.
/// #   pub fn verify() -> impl Middleware<()> { Verify }
/// #
/// #   struct Restore;
/// #   struct Verify;
/// #
/// #   impl Middleware<()> for Restore {
/// #       fn call(&self, request: Request, next: Next) -> BoxFuture { todo!() }
/// #   }
/// #
/// #   impl Middleware<()> for Verify {
/// #       fn call(&self, request: Request, next: Next) -> BoxFuture { todo!() }
/// #   }
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<ExitCode, Error> {
///     // Create a new application.
///     let mut app = via::app(());
///
///     // Cookies are parsed and managed for every request.
///     app.middleware(cookies([session::COOKIE]));
///
///     // Define the /api namespace. Middleware attached to
///     // `api` only runs for requests to a nested resource
///     // (i.e /api/users).
///     let mut api = app.route("/api");
///
///     // Errors originating from the /api namespace generate a
///     // JSON response.
///     api.middleware(rescue(|error| error.use_json()));
///
///     // Confirm the client speaks JSON. Then, restore their
///     // session.
///     //
///     // We call this pattern a "side car". A guard paired with
///     // middleware that only executes when the guard succeeds.
///     //
///     // In addition to eliminating the cost of the business
///     // logic in the subsequent middleware when the guard
///     // fails, the framework over head of the attached
///     // middleware is completely erased.
///     //
///     // It is a best practice to give a guard a side car when
///     // it makes sense. For example, we wouldn't want to
///     // restore a user's session to our JSON API if they cannot
///     // send and receive JSON.
///     api.middleware(guard::flat_map(
///         guard::content!(media::json),
///         session::restore(),
///     ));
///
///     // If the request method cannot be cached or the session
///     // has not been verified in the past hour, confirm that
///     // the user exists and has an active account.
///     api.middleware(guard::filter(
///         guard::or((method::is_mutation(), session::is_expired)),
///         session::verify(),
///     ));
///
///     // Serve the application at http://localhost:8080/.
///     Server::new(app).listen(("127.0.0.1", 8080)).await
/// }
/// ```
///
/// In the example above downstream middleware may never observe session state
/// at all if guards fail early.
///
/// - `session::restore` is only executed if content negotiation succeeds
///
/// - `session::verify` is skipped for safe requests with fresh sessions
///
/// This pattern is especially powerful when composing multiple stateless
/// guards, since each guard can eliminate not just itself, but all nested
/// middleware beneath it. Middleware stacks can be expressed declaratively
/// while still achieving branch-local execution costs.
///
/// In practice, this means:
///
/// - fewer allocations in request hot paths
///
/// - fewer futures constructed per request (less dynamic dispatch)
///
/// - early rejection of work before async boundaries are crossed (the
///   adversarial window of opportunity is reduced)
///
/// However, this should not be used as a substitute for correctness or
/// clarity. Guards should be chosen primarily for expressiveness of request
/// constraints, with performance benefits treated as a consequence of correct
/// structure rather than the primary design goal.
///
/// ## Stateful Middleware
///
/// Sometimes middleware needs to perform asynchronous work before the request
/// is forwarded to the next middleware in the chain or after a response is
/// built by a terminal middleware. Examples of stateful middleware include:
///
/// - A request logger that includes the status code of the response
///
/// - Authentication middleware that queries a database to confirm that a
///   user's account is active
///
/// - Cookie middleware parses request cookies and tracks changes to generate
///   `set-cookie` headers from the delta
///
/// In these examples, the middleware needs to `.await` at least one other
/// future. To achieve this, we must return our own async block inside a box
/// future and drive the next middleware from the inside.
///
/// ```
/// use std::io::Write;
/// use std::sync::Arc;
/// use tokio::task::spawn_blocking;
/// use via::{BoxFuture, Middleware, Next, Request};
///
/// /// Logs the method, path, and status to an `impl Write`.
/// struct Tap<T> {
///     io: Arc<T>,
/// }
///
/// impl<T, Io, App> Middleware<App> for Tap<T>
/// where
///     T: Fn() -> Io + Send + Sync + 'static,
///     Io: Write,
///     App: Send + Sync + 'static,
/// {
///     fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
///         // Clone `io` so it can move into the log task.
///         let io = Arc::clone(&self.io);
///
///         // Return our own future so we can await the response.
///         Box::pin(async {
///             // Clone the log task dependencies from request.
///             let path = request.uri().path().to_owned();
///             let method = request.method().clone();
///
///             // Call the next middleware to get a response.
///             let response = next.call(request).await?;
///             //                                     ^
///             // The try operator is safe so long as `rescue`
///             // occurs after `tap` in the main fn of your app.
///
///             // Get a copy of the response status code.
///             let status = response.status();
///
///             // Log in a detached background task.
///             spawn_blocking(move || {
///                 let _ = writeln!(
///                     &mut io(),
///                     "{} {} ~> {}",
///                     method, path, status
///                 );
///             });
///
///             // Send the response to the client.
///             Ok(response)
///         })
///     }
/// }
/// ```
///
/// While stateful middleware is more flexible because it allows middleware to
/// run code both before and after the remainder of the resolved route stack is
/// executed, it comes at a cost. The tradeoff is that stateful middleware must
/// create another future to hold its state. In practice this means an
/// additional allocation and dynamic-dispatch call.
///
/// For most applications, this cost is insignificant. Network I/O, database
/// queries, TLS handshakes, and interacting asynchronously with a remote
/// resource dominates the latency balance sheet. Even so, if your middleware
/// can be implemented synchronously, it is generally simpler and slightly more
/// efficient to do so.
///
/// ## Choosing a Style
///
/// The distinction is more about structure than capability. Stateless
/// middleware performs all of its work before delegating to downstream
/// middleware. Stateful middleware retains state across an asynchronous
/// boundary, allowing it to observe or modify the result produced by
/// downstream middleware.
///
/// As a general rule of thumb:
///
/// - Use terminal middleware to generate responses with an `async fn`
///
/// - Use stateless middleware when all work can be performed synchronously
///
/// - Use stateful middleware when work must cross an `.await` boundary, when
///   you need to inspect the response produced by downstream middleware
///
/// - Prefer explicitly implementing `Middleware` for a type when you need a
///   type name to support composition or have an opinion about the memory
///   layout of captured state
///
/// [guards]: mod@crate::guard
pub trait Middleware<App>: Send + Sync {
    /// Respond to `request` or call the `next` middleware.
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture;
}

/// Define middleware as a function that may capture its environment.
///
/// The blanket implementation of `Middleware` is intentionally limited to fn
/// pointers and non-capturing closures. This keeps middleware lightweight,
/// while preventing accidental captures from becoming long-lived state.
///
/// Unlike the blanket implementation, the closure passed to middleware may
/// capture state from its environment and it must return a [`BoxFuture`]
/// directly. This means the closure decides where the future is boxed. If the
/// closure can delegate directly to `next`, it may do so without wrapping
/// downstream execution in an additional future.
///
/// # Example
///
/// ```
/// use std::io::Write;
/// use std::sync::Arc;
/// use tokio::task::spawn_blocking;
/// use via::{Middleware, middleware};
///
/// /// Logs the method, path, and status to `Io`.
/// fn tap<T, App, Io>(io: T) -> impl Middleware<App> + 'static
/// where
///     T: Fn() -> Io + Send + Sync + 'static,
///     App: Send + Sync + 'static,
///     Io: Write,
/// {
///     // Wrap `io` in an Arc so it can move into the log task.
///     let io = Arc::new(io);
///
///     // Capture `io` as an anonymous field.
///     middleware(move |request, next| {
///         // Clone `io` so it can move into the log task.
///         let io = Arc::clone(&io);
///
///         // Return our own future so we can await the response.
///         Box::pin(async {
///             // Clone the log task dependencies from request.
///             let path = request.uri().path().to_owned();
///             let method = request.method().clone();
///
///             // Call the next middleware to get a response.
///             let response = next.call(request).await?;
///             //                                     ^
///             // The try operator is safe so long as `rescue`
///             // occurs after `tap` in the main fn of your app.
///
///             // Get a copy of the response status code.
///             let status = response.status();
///
///             // Log in a detached background task.
///             spawn_blocking(move || {
///                 let _ = writeln!(
///                     &mut io(),
///                     "{} {} ~> {}",
///                     method, path, status
///                 );
///             });
///
///             // Send the response to the client.
///             Ok(response)
///         })
///     })
/// }
/// ```
pub fn middleware<T, App>(call: T) -> impl Middleware<App> + 'static
where
    T: Fn(Request<App>, Next<App>) -> BoxFuture + Send + Sync + 'static,
{
    MiddlewareFn { middleware: call }
}

struct MiddlewareFn<T> {
    middleware: T,
}

impl<T, App> Middleware<App> for MiddlewareFn<T>
where
    T: Fn(Request<App>, Next<App>) -> BoxFuture + Send + Sync,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        (self.middleware)(request, next)
    }
}

impl<T, Await, App> Middleware<App> for T
where
    T: Fn(Request<App>, Next<App>) -> Await + Copy + Send + Sync,
    Await: Future<Output = Result> + Send + 'static,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        Box::pin(self(request, next))
    }
}
