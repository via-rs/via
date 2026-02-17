use std::sync::Arc;

use crate::app::Shared;
use crate::request::Envelope;

#[derive(Debug)]
pub struct Request<App> {
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
        let (envelope, _, app) = request.into_parts();

        Self {
            envelope: Arc::new(envelope),
            app,
        }
    }

    pub(super) fn envelope_mut(&mut self) -> &mut Arc<Envelope> {
        &mut self.envelope
    }
}

impl<App> Clone for Request<App> {
    fn clone(&self) -> Self {
        Self {
            envelope: Arc::clone(&self.envelope),
            app: self.app.clone(),
        }
    }
}
