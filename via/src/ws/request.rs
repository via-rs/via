use hyper::upgrade::OnUpgrade;
use std::sync::Arc;

use crate::app::Shared;
use crate::request::Envelope;

#[derive(Debug)]
pub struct Request<App = ()> {
    pub(super) on_upgrade: Option<OnUpgrade>,
    envelope: Arc<Envelope>,
    app: Shared<App>,
}

impl<App> Request<App> {
    pub fn app(&self) -> &App {
        &self.app
    }

    pub fn envelope(&self) -> &Envelope {
        &self.envelope
    }

    pub fn app_owned(&self) -> Shared<App> {
        self.app.clone()
    }
}

impl<App> Request<App> {
    pub(super) fn new(request: crate::Request<App>) -> Self {
        let (mut envelope, _, app) = request.into_parts();

        Self {
            on_upgrade: envelope.extensions_mut().remove(),
            envelope: Arc::new(envelope),
            app,
        }
    }
}

impl<App> Clone for Request<App> {
    fn clone(&self) -> Self {
        Self {
            on_upgrade: None,
            envelope: Arc::clone(&self.envelope),
            app: self.app.clone(),
        }
    }
}
