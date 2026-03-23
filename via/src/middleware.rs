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

impl<T, Await, App> Middleware<App> for T
where
    T: Fn(Request<App>, Next<App>) -> Await + Send + Sync,
    Await: Future<Output = Result> + Send + 'static,
{
    fn call(&self, request: Request<App>, next: Next<App>) -> BoxFuture {
        Box::pin(self(request, next))
    }
}
