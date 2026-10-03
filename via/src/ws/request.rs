use delegate::delegate;
use http::request::Parts;
use http::{Extensions, Uri};
use std::sync::Arc;

use crate::app::Shared;
use crate::error::Error;
use crate::request::params::{self, PathParam, PathParams, QueryParams};

#[derive(Debug)]
pub struct Request<App = ()> {
    envelope: Arc<Envelope<App>>,
}

#[derive(Debug)]
struct Envelope<App> {
    extensions: Extensions,
    uri: Uri,
    params: Vec<via_router::PathParam>,
    app: Shared<App>,
}

impl<App> Request<App> {
    delegate! {
        to self.envelope() {
            /// Returns a reference to the associated extensions.
            pub fn extensions(&self) -> &Extensions;

            /// Returns a reference to the request's URI.
            pub fn uri(&self) -> &Uri;

            /// Returns reference to the second argument passed to [`Server::new`].
            ///
            /// [`Server::new`]: crate::Server::new
            pub fn app(&self) -> &App;

            /// Returns an owned, reference-counting pointer to the second argument
            /// passed to [`Server::new`].
            ///
            /// [`Server::new`]: crate::Server::new
            pub fn app_owned(&self) -> Shared<App>;
        }
    }

    /// Returns a convenient wrapper around an optional reference to
    /// the path parameter in the request's uri with the provided `name`.
    pub fn param<'b>(&self, name: &'b str) -> PathParam<'_, 'b> {
        let envelope = self.envelope();
        let source = envelope.uri().path();
        let param = params::get(envelope.params(), name);

        PathParam::new(source, param, name)
    }

    pub fn query<'a, T>(&'a self) -> crate::Result<T>
    where
        T: TryFrom<QueryParams<'a>, Error = Error>,
    {
        let query = self.envelope().uri().query();
        T::try_from(QueryParams::new(query))
    }

    pub fn params<'a, T>(&'a self) -> crate::Result<T>
    where
        T: TryFrom<PathParams<'a>>,
        Error: From<T::Error>,
    {
        let envelope = self.envelope();

        Ok(T::try_from(PathParams::new(
            envelope.uri().path(),
            envelope.params(),
        ))?)
    }

    fn envelope(&self) -> &Envelope<App> {
        &self.envelope
    }
}

impl<App> Request<App> {
    pub(crate) fn new(parts: Parts, params: Vec<via_router::PathParam>, app: Shared<App>) -> Self {
        Self {
            envelope: Arc::new(Envelope {
                extensions: parts.extensions,
                uri: parts.uri,
                params,
                app,
            }),
        }
    }
}

impl<App> Clone for Request<App> {
    fn clone(&self) -> Self {
        Self {
            envelope: Arc::clone(&self.envelope),
        }
    }
}

impl<App> Envelope<App> {
    fn app_owned(&self) -> Shared<App> {
        self.app.clone()
    }

    fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    fn params(&self) -> &[via_router::PathParam] {
        &self.params
    }

    fn uri(&self) -> &Uri {
        &self.uri
    }

    fn app(&self) -> &App {
        &self.app
    }
}
