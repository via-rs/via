use cookie::CookieJar;
use delegate::delegate;
use http::{Extensions, HeaderMap, Method, Uri, Version};
use hyper::upgrade::OnUpgrade;
use std::sync::Arc;

use crate::app::Shared;
use crate::error::Error;
use crate::request::params::PathParam;
use crate::request::{Envelope, PathParams, QueryParams};

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

    pub fn app_owned(&self) -> Shared<App> {
        self.app.clone()
    }

    delegate! {
        to self.envelope {
            /// Returns a reference to the request's method.
            pub fn method(&self) -> &Method;

            /// Returns a reference to the request's URI.
            pub fn uri(&self) -> &Uri;

            /// Returns the HTTP version that was used to make the request.
            pub fn version(&self) -> Version;

            /// Returns a reference to the request's headers.
            pub fn headers(&self) -> &HeaderMap;

            /// Returns reference to the cookies associated with the request.
            pub fn cookies(&self) -> &CookieJar;

            /// Returns a reference to the associated extensions.
            pub fn extensions(&self) -> &Extensions;

            /// Returns a convenient wrapper around an optional reference to
            /// the path parameter in the request's uri with the provided `name`.
            pub fn param<'b>(&self, name: &'b str) -> PathParam<'_, 'b>;

            pub fn query<'a, T>(&'a self) -> crate::Result<T>
            where
                T: TryFrom<QueryParams<'a>, Error = Error>;

            pub fn params<'a, T>(&'a self) -> crate::Result<T>
            where
                T: TryFrom<PathParams<'a>>,
                Error: From<T::Error>;
        }
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
