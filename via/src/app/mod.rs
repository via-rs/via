mod shared;

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
    #[inline]
    pub(crate) fn into_parts(self) -> (Router<App>, Shared<App>) {
        (self.router, self.app)
    }
}
