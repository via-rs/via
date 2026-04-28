pub mod params;

mod payload;
mod query;

pub use params::{PathParams, QueryParams};
pub use payload::{Aggregate, Coalesce, Payload, RequestBody};

use cookie::CookieJar;
use delegate::delegate;
use http::request::Parts;
use http::{Extensions, HeaderMap, Method, Uri, Version};
use std::fmt::{self, Debug, Formatter};

use crate::ResultExt;
use crate::app::Shared;
use crate::error::Error;
use crate::response::{Finalize, Response, ResponseBuilder};
use params::PathParam;

pub struct Envelope {
    parts: Parts,
    params: Vec<via_router::PathParam>,
    cookies: CookieJar,
}

pub struct Request<App = ()> {
    envelope: Envelope,
    body: RequestBody,
    app: Shared<App>,
}

impl Envelope {
    /// Returns a reference to the request's method.
    ///
    #[inline]
    pub fn method(&self) -> &Method {
        &self.parts.method
    }

    /// Returns a reference to the request's URI.
    ///
    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.parts.uri
    }

    /// Returns the HTTP version that was used to make the request.
    ///
    #[inline]
    pub fn version(&self) -> Version {
        self.parts.version
    }

    /// Returns a reference to the request's headers.
    ///
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.parts.headers
    }

    /// Returns reference to the cookies associated with the request.
    ///
    #[inline]
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }

    /// Returns a mutable reference to the cookies associated with the request.
    ///
    #[inline]
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }

    /// Returns a reference to the associated extensions.
    ///
    #[inline]
    pub fn extensions(&self) -> &Extensions {
        &self.parts.extensions
    }

    /// Returns a mutable reference to the associated extensions.
    ///
    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.parts.extensions
    }

    /// Returns a convenient wrapper around an optional reference to the path
    /// parameter in the request's uri with the provided `name`.
    ///
    pub fn param<'b>(&self, name: &'b str) -> PathParam<'_, 'b> {
        let param = params::get(&self.params, name);
        PathParam::new(self.uri().path(), param, name)
    }

    pub fn query<'a, T>(&'a self) -> crate::Result<T>
    where
        T: TryFrom<QueryParams<'a>, Error = Error>,
    {
        T::try_from(QueryParams::new(self.uri().query()))
    }

    pub fn params<'a, T>(&'a self) -> crate::Result<T>
    where
        T: TryFrom<PathParams<'a>>,
        Error: From<T::Error>,
    {
        let path = self.uri().path();
        let params = &self.params;

        T::try_from(PathParams::new(path, params)).or_bad_request()
    }
}

impl Envelope {
    #[inline]
    pub(crate) fn new(parts: Parts, params: Vec<via_router::PathParam>) -> Self {
        Self {
            parts,
            params,
            cookies: CookieJar::new(),
        }
    }
}

impl Debug for Envelope {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        #[derive(Debug)]
        struct CookieJar;

        f.debug_struct("Envelope")
            .field("method", self.method())
            .field("uri", self.uri())
            .field("params", &self.params)
            .field("version", &self.version())
            .field("headers", self.headers())
            .field("cookies", &CookieJar)
            .field("extensions", self.extensions())
            .finish()
    }
}

impl<App> Request<App> {
    #[inline]
    pub(crate) fn new(envelope: Envelope, body: RequestBody, app: Shared<App>) -> Self {
        Self {
            envelope,
            body,
            app,
        }
    }

    #[inline]
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

            /// Returns a mutable reference to the cookies associated with the request.
            pub fn cookies_mut(&mut self) -> &mut CookieJar;

            /// Returns a reference to the associated extensions.
            pub fn extensions(&self) -> &Extensions;

            /// Returns a mutable reference to the associated extensions.
            pub fn extensions_mut(&mut self) -> &mut Extensions;

            /// Returns reference to the cookies associated with the request.
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

    /// Consumes the request and returns a tuple containing a future that
    /// resolves with the data and trailers of the body as well as a shared
    /// copy of `App`.
    ///
    pub fn into_future(self) -> (Coalesce, Shared<App>) {
        (Coalesce::new(self.body), self.app)
    }

    /// Consumes the request and returns a tuple containing it's parts.
    ///
    #[inline]
    pub fn into_parts(self) -> (Envelope, RequestBody, Shared<App>) {
        (self.envelope, self.body, self.app)
    }
}

impl<App> Debug for Request<App> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Request")
            .field("envelope", &self.envelope)
            .field("body", &self.body)
            .field("app", &self.app)
            .finish()
    }
}

impl<App> Finalize for Request<App> {
    fn finalize(self, response: ResponseBuilder) -> Result<Response, Error> {
        use http::header::{CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING};
        use http_body_util::combinators::BoxBody;

        let headers = self.headers();

        let mut response = match headers.get(CONTENT_LENGTH).cloned() {
            Some(content_length) => response.header(CONTENT_LENGTH, content_length),
            None => response.header(TRANSFER_ENCODING, "chunked"),
        };

        if let Some(content_type) = headers.get(CONTENT_TYPE).cloned() {
            response = response.header(CONTENT_TYPE, content_type);
        }

        response.body(BoxBody::new(self.body).into())
    }
}
