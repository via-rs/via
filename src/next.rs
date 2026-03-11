use std::collections::VecDeque;
use std::sync::Arc;

use crate::middleware::{BoxFuture, Middleware};
use crate::request::Request;

/// A no-op middleware that simply calls the next middleware in the stack.
///
/// `Continue` acts as a neutral element in middleware composition. It performs
/// no work of its own and immediately forwards the request to `next`.
///
/// Although it may appear trivial, `Continue` is a useful building block for
/// implementing middleware combinators that provide custom branching logic
/// where a concrete fallback is required.
pub struct Continue;

/// The next middleware in the logical call stack of a request.
pub struct Next<App = ()> {
    deque: VecDeque<Arc<dyn Middleware<App>>>,
}

impl<App> Middleware<App> for Continue {
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        next.call(request)
    }
}

impl<App> Next<App> {
    #[inline]
    pub(crate) fn new(deque: VecDeque<Arc<dyn Middleware<App>>>) -> Self {
        Self { deque }
    }

    /// Calls the next middleware in the logical call stack of the request.
    ///
    /// # Example
    ///
    /// ```
    /// use via::{Request, Next};
    ///
    /// async fn logger(request: Request, next: Next) -> via::Result {
    ///     let head = request.envelope();
    ///
    ///     println!("{} -> {}", head.method(), head.uri().path());
    ///     next.call(request).await.inspect(|response| {
    ///         println!("<- {}", response.status());
    ///     })
    /// }
    /// ```
    pub fn call(mut self, request: Request<App>) -> BoxFuture {
        match self.deque.pop_front() {
            Some(middleware) => middleware.call(request, self),
            None => Box::pin(async { crate::raise!(404) }),
        }
    }
}

// Explicitly impl Drop to make a supply-chain risk a build-time error.
//
// Rationale:
//
// A malicious crate in the supply chain could `impl Drop for Next` and call
// the remaining middleware in the deque to see the outcome of a rejected
// request.
impl<App> Drop for Next<App> {
    fn drop(&mut self) {}
}
