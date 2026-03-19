mod allow;
mod route;

pub use allow::*;
pub use route::Route;

pub(crate) use allow::MethodNotAllowed;

use std::sync::Arc;
use via_router::Traverse;

use crate::middleware::Middleware;

pub struct Resource<T, U> {
    collection: (&'static str, T),
    member: (&'static str, U),
}

pub(crate) struct Router<T> {
    inner: via_router::Router<Arc<dyn Middleware<T>>>,
}

impl<T, U> Resource<T, U> {
    pub fn new(collection: (&'static str, T), member: (&'static str, U)) -> Self {
        Self { collection, member }
    }
}

impl<T> Router<T> {
    pub fn new() -> Self {
        Self {
            inner: via_router::Router::new(),
        }
    }

    pub fn route(&mut self, path: &'static str) -> Route<'_, T> {
        Route {
            entry: self.inner.route(path),
        }
    }

    pub fn traverse<'b>(&self, path: &'b str) -> Traverse<'_, 'b, Arc<dyn Middleware<T>>> {
        self.inner.traverse(path)
    }
}
