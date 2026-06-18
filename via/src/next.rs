use std::collections::VecDeque;
use std::sync::Arc;

use crate::err;
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

/// A linear, single-use execution cursor over middleware.
///
/// Middleware receives ownership of `next` and may either delegate to the
/// subsequent middleware in the deque or build a response and terminate
/// execution altogether.
///
/// `Next` has strict ownership semantics with a consuming API. When the "next"
/// middleware is called, ownership of `self` and `request` are transferred to
/// the middleware popped from the front of the deque. If the deque is empty,
/// `Next` returns a `404 Not Found` error.
///
/// Because `Next` owns the remaining middleware chain, choosing not to call it
/// is how middleware terminates or rejects a request. This makes control flow
/// explicit: downstream middleware only execute when an upstream middleware
/// delegates to them.
///
/// `Next` cannot be cloned or reused. This prevents middleware from executing
/// the same downstream chain more than once, speculatively observing rejected
/// requests, or continuing execution after a terminal middleware has already
/// produced a response.
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

    /// Call the "next" middleware in the logical call stack for the request.
    ///
    /// # Example
    ///
    /// ```
    /// use via::{Middleware, middleware};
    ///
    /// /// Forwards `request` to the "next" middleware.
    /// fn forward<App>() -> impl Middleware<App> + 'static {
    ///     middleware(|request, next| next.call(request))
    /// }
    /// ```
    pub fn call(mut self, request: Request<App>) -> BoxFuture {
        match self.deque.pop_front() {
            Some(middleware) => middleware.call(request, self),
            None => {
                let error = err!(404, "not found.");
                Box::pin(async { Err(error) })
            }
        }
    }
}

/// Explicitly implement Drop to make a supply-chain risk a build-time error.
//
// Rationale:
//
// A malicious crate in the supply chain could `impl Drop for Next` and call
// the remaining middleware in the deque to see the outcome of a rejected
// request.
impl<App> Drop for Next<App> {
    fn drop(&mut self) {}
}
