mod service;
mod shared;

pub(crate) use service::ServiceAdapter;
pub use shared::Shared;

use crate::router::Router;

pub(crate) struct Via<App> {
    router: Router<App>,
    app: Shared<App>,
}

impl<App> Via<App> {
    pub(crate) fn new(router: Router<App>, app: App) -> Self {
        Self {
            router,
            app: Shared::new(app),
        }
    }
}

impl<App> Via<App> {
    fn app(&self) -> &Shared<App> {
        &self.app
    }

    fn router(&self) -> &Router<App> {
        &self.router
    }
}
