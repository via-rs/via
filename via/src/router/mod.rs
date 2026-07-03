mod route;
mod switch;

pub use route::Route;
pub use switch::*;

pub(crate) use switch::MethodNotAllowed;

use std::sync::Arc;
use via_router::Traverse;

use crate::middleware::Middleware;

pub(crate) struct Router<T> {
    tree: via_router::Router<Arc<dyn Middleware<T>>>,
}

impl<App> Router<App> {
    pub fn new() -> Self {
        Self {
            tree: via_router::Router::new(),
        }
    }

    pub fn push(&mut self, path: &'static str) -> Route<'_, App> {
        Route(self.tree.route(path))
    }

    pub fn route<T>(&mut self, path: &'static str, middleware: T) -> Route<'_, App>
    where
        T: Middleware<App> + 'static,
    {
        self.push(path).assign(middleware)
    }

    pub fn traverse<'b>(&self, path: &'b str) -> Traverse<'_, 'b, Arc<dyn Middleware<App>>> {
        self.tree.traverse(path)
    }
}
