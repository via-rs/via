mod allow;
mod route;

pub use allow::*;
pub use route::{Index, Resource, ResourceBuilder, Route};

pub(crate) use allow::MethodNotAllowed;

use std::sync::Arc;
use via_router::Traverse;

use crate::middleware::Middleware;

pub(crate) struct Router<T> {
    tree: via_router::Router<Arc<dyn Middleware<T>>>,
}

impl<T> Router<T> {
    pub fn new() -> Self {
        Self {
            tree: via_router::Router::new(),
        }
    }

    pub fn route(&mut self, path: &'static str) -> Route<'_, T> {
        Route(self.tree.route(path))
    }

    pub fn traverse<'b>(&self, path: &'b str) -> Traverse<'_, 'b, Arc<dyn Middleware<T>>> {
        self.tree.traverse(path)
    }
}
