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

pub trait Middleware<App>: Send + Sync {
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture;
}

/// Define middleware as a function that may capture its environment.
///
/// The blanket implementation of `Middleware` is intentionally limited to fn
/// pointers and non-capturing closures. This keeps middleware lightweight,
/// while preventing accidental captures from becoming long-lived state.
///
/// Unlike the blanket implementation, the closure passed to middleware may
/// capture state from it's environment and it must return a [`BoxFuture`]
/// directly. This means the closure decides where the future is boxed. If the
/// `call` argument can delegate directly to `next`, it may do so without
/// wrapping downstream execution in an additional future.
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
