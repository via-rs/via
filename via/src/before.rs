use std::ops::ControlFlow;

use crate::error::Catch;
use crate::{BoxFuture, Middleware, Next, Request};

/// A middleware adapter that decorates a request before delegation.
///
/// `Before` runs a synchronous decorator function against the incoming
/// [`Request`] before invoking another middleware.
///
/// The decorator may mutate the request in place to derive, normalize, or cache
/// request state for downstream middleware. This is useful for work such as
/// restoring session identity, parsing cookies, or initializing extensions from
/// request metadata.
///
/// Decorators return [`Catch`], which determines how decoration failures affect
/// the pipeline:
///
/// - [`ControlFlow::Break`] terminates execution and returns the error.
/// - [`ControlFlow::Continue`] logs the error, skips the wrapped middleware,
///   and continues with [`Next`].
///
/// See [`before`] for a convenient constructor.
pub struct Before<T, U> {
    decorator: T,
    middleware: U,
}

/// Creates middleware that decorates a request before invoking another
/// middleware.
///
/// The `decorator` receives a mutable reference to the request and may mutate
/// it in place before the wrapped middleware executes.
///
/// If decoration succeeds, the wrapped middleware is called. If decoration
/// fails with [`ControlFlow::Break`], the error is returned immediately. If it
/// fails with [`ControlFlow::Continue`], the error is logged and the request is
/// forwarded to the next middleware without calling the wrapped middleware.
///
/// # Example
///
/// Restore an identity token from the session cookie before conditionally
/// refreshing the active user session.
///
/// ```no_run
/// mod session {
///     // Implementations elided...
///     # use via::error::Catch;
///     # use via::guard::Predicate;
///     # use via::{Middleware, Request};
///     #
///     # use super::Buttercup;
///     #
///     # pub const COOKIE: &str = "via-session";
///     #
///     # pub fn needs_verified() -> fn(&Request<Buttercup>) -> bool { |_| true }
///     # pub fn restore(_: &mut Request<Buttercup>) -> Result<(), Catch> { todo!() }
///     # pub fn verify() -> impl Middleware<Buttercup> + 'static {
///     #     via::middleware(|request, next| next.call(request))
///     # }
/// }
///
/// use cookie::Key;
/// use std::process::ExitCode;
/// use via::guard::{self, media, method};
/// use via::{Router, Server, cookies, rescue};
///
/// struct Buttercup {
///     secret: Key,
/// }
///
/// #[tokio::main]
/// async fn main() -> via::Result<ExitCode> {
///     // Define the routes that our application responds to.
///     let router = Router::new(|home| {
///         // Start defining descendants of "/".
///         let mut path = home.prefix();
///
///         // The /api namespace.
///         let mut api = path.push("api");
///
///         // If an error occurs, respond with JSON.
///         api.middleware(rescue::json().build());
///
///         // Parse and track changes that are made to the session cookie.
///         api.middleware(cookies([session::COOKIE]));
///
///         // Content negotiation and authentication guards.
///         api.middleware(guard::flat_map(
///             // Confirm that the client speaks JSON.
///             guard::content!(media::json()),
///             // Then, initialize the active user session.
///             via::before(
///                 // Restore an identity token from the session cookie.
///                 session::restore,
///                 // If the request is read only or the active users account has
///                 // been confirmed to exist in the past hour, skip verification.
///                 //
///                 // Such an optimization is sound so long as your app is the
///                 // authoritative source of truth for the session and you
///                 // properly end websocket sessions when a user deletes their
///                 // account.
///                 guard::filter(
///                     guard::or((method::is_mutation(), session::needs_verified())),
///                     session::verify(),
///                 ),
///             ),
///         ));
///
///         // Start defining descendants of "/api".
///         let mut path = api.prefix();
///     });
///
///     // Setup our application, "Buttercup".
///     let buttercup = Buttercup {
///         secret: Key::generate(),
///         // secret: std::env::var("VIA_SECRET_KEY")
///         //     .map(|secret| secret.as_bytes().try_into())
///         //     .expect("missing required env var: VIA_SECRET_KEY")
///         //     .expect("unexpected end of input while parsing VIA_SECRET_KEY"),
///     };
///
///     // Start listening at http://localhost:8080 for incoming requests.
///     Server::new(router, buttercup)
///         .listen(("127.0.0.1", 8080))
///         .await
///}
/// ```
///
/// <details>
/// <summary>Click here to view an example <code>mod session</code>.</summary>
///
/// ```rust
/// # use cookie::Key;
/// #
/// # struct Unicorn {
/// #     secret: Key,
/// # }
/// #
/// # impl Unicorn {
/// #     fn secret(&self) -> &Key { &self.secret }
/// # }
/// #
/// use via::error::{Catch, Error, Propagate};
/// use via::{Middleware, Request, err};
///
/// pub const COOKIE: &str = "via-session";
///
/// #[derive(Clone, Copy, PartialEq)]
/// pub struct Identity([u8; 16]);
///
/// impl std::str::FromStr for Identity {
///     type Err = Error;
///
///     fn from_str(input: &str) -> Result<Self, Self::Err> {
///          todo!()
///     }
/// }
///
/// /// Restore the active user session from the session cookie.
/// pub fn restore(request: &mut Request<Unicorn>) -> Result<(), Catch> {
///     // Authenticate the signed cookie jar using the signing secret
///     // stored in the application (Unicorn).
///     let jar = {
///         let secret = request.app().secret();
///         request.cookies().signed(secret)
///     };
///
///     let token = jar
///         // Retrieve the session cookie from the signed cookie jar.
///         .get(COOKIE)
///         // If the session cookie *is not* present, return a generic,
///         // `401 Unauthorized` error.
///         .ok_or_else(|| err!(401, "unauthorized."))
///         // If the session cookie *is* present, parse an identity token
///         // from the base64 encoded string in its value.
///         .and_then(|cookie| cookie.value().parse::<Identity>())
///         // If an error occurred, continue to the next middleware.
///         // The user may be trying to create an account or login.
///         .or_continue()?;
///
///     // Insert the identity token into the request extensions.
///     request.extensions_mut().insert(token);
///
///     // The request was decorated successfully.
///     Ok(())
/// }
///
/// /// Returns a middleware that verifies the active user account.
/// pub fn verify() -> impl Middleware<Unicorn> + 'static {
///     via::middleware(|request, next| {
///         todo!("verify that the active user has an account.")
///     })
/// }
/// ```
/// </details>
#[inline(always)]
pub fn before<T, U>(decorator: T, middleware: U) -> Before<T, U> {
    Before {
        decorator,
        middleware,
    }
}

impl<T, U, App> Middleware<App> for Before<T, U>
where
    T: Fn(&mut Request<App>) -> Result<(), Catch> + Copy + Send + Sync,
    U: Middleware<App>,
{
    fn call(&self, mut request: Request<App>, next: Next<App>) -> BoxFuture {
        match (self.decorator)(&mut request) {
            Ok(_) => self.middleware.call(request, next),
            Err(ControlFlow::Break(error)) => Box::pin(async { Err(error) }),
            Err(ControlFlow::Continue(error)) => {
                log!(warn(before = 0), "{}", &error);
                next.call(request)
            }
        }
    }
}
