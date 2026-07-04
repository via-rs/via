mod route;
mod switch;

pub use route::Route;
pub use switch::*;

pub(crate) use switch::MethodNotAllowed;

use std::sync::Arc;
use via_router::Traverse;

use crate::middleware::Middleware;

pub(crate) struct Router<App> {
    tree: via_router::Router<Arc<dyn Middleware<App>>>,
}

impl<App> Router<App> {
    pub fn new() -> Self {
        Self {
            tree: via_router::Router::new(),
        }
    }

    pub fn middleware<T>(&mut self, middleware: T)
    where
        T: Middleware<App> + 'static,
    {
        self.tree.middleware(Arc::new(middleware))
    }

    pub fn push(&mut self, path: &'static str) -> Route<'_, App> {
        Route {
            node: self.tree.route(path),
        }
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
