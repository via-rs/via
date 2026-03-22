use crate::middleware::BoxFuture;
use crate::{Middleware, Next, Request};

/// Stop processing the request and respond if the provided precondition fails.
///
/// Guard wraps a synchronous check function `T` that receives a reference to
/// the incoming request and returns a result.
///
/// This is useful for lightweight, synchronous validations that do not require
/// async work—such as authorization checks based on headers, request metadata,
/// or other inexpensive predicates.
///
/// # Example
///
/// ```rust
/// use via::{Guard, Request, raise};
///
/// let mut app = via::app(());
///
/// app.route("/users").scope(|path| {
///     // Subsequently defined routes require a valid API key.
///     path.middleware(Guard::new(validate_api_key));
/// });
///
/// fn validate_api_key(request: &Request) -> via::Result<()> {
///     let Some(key) = request.envelope().headers().get("x-api-key") else {
///         raise!(401, message = "missing required header: x-api-key");
///     };
///
///     // Insert API key validation here...
///
///     Ok(())
/// }
/// ```
pub struct Guard<T> {
    check: T,
}

impl<T> Guard<T> {
    /// Returns a new guard that uses the provided check function.
    pub fn new(check: T) -> Self {
        Self { check }
    }
}

impl<T, App> Middleware<App> for Guard<T>
where
    T: Fn(&Request<App>) -> crate::Result<()> + Send + Sync,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        if let Err(error) = (self.check)(&request) {
            Box::pin(async { Err(error) })
        } else {
            next.call(request)
        }
    }
}
