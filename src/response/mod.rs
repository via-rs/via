mod body;
mod builder;
mod redirect;

#[cfg(feature = "file")]
mod file;

pub use body::{Json, ResponseBody};
pub use builder::{Finalize, ResponseBuilder};
pub use redirect::Redirect;

#[cfg(feature = "file")]
pub use file::File;

use bytes::Bytes;
use cookie::CookieJar;
use http::{Extensions, HeaderMap, StatusCode, Version};
use http_body::Body;
use http_body_util::combinators::BoxBody;
use std::fmt::{self, Debug, Formatter};

use crate::error::BoxError;

pub struct Response {
    http: http::Response<ResponseBody>,
    cookies: CookieJar,
}

impl Response {
    #[inline]
    pub fn build() -> ResponseBuilder {
        Default::default()
    }

    #[inline]
    pub fn new(body: ResponseBody) -> Self {
        Self {
            cookies: CookieJar::new(),
            http: http::Response::new(body),
        }
    }

    /// Consumes the response returning a new response with body mapped to the
    /// return type of the provided closure.
    ///
    #[inline]
    pub fn map<U, F>(self, map: F) -> Self
    where
        F: FnOnce(ResponseBody) -> U,
        U: Body<Data = Bytes, Error = BoxError> + Send + Sync + 'static,
    {
        Self {
            cookies: self.cookies,
            http: self.http.map(|body| BoxBody::new(map(body)).into()),
        }
    }

    #[inline]
    pub fn status(&self) -> StatusCode {
        self.http.status()
    }

    #[inline]
    pub fn status_mut(&mut self) -> &mut StatusCode {
        self.http.status_mut()
    }

    #[inline]
    pub fn version(&self) -> Version {
        self.http.version()
    }

    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        self.http.headers()
    }

    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        self.http.headers_mut()
    }

    /// Returns a reference to the response cookies.
    ///
    #[inline]
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }

    /// Returns a mutable reference to the response cookies.
    ///
    #[inline]
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }

    #[inline]
    pub fn extensions(&self) -> &Extensions {
        self.http.extensions()
    }

    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        self.http.extensions_mut()
    }

    #[inline]
    pub fn body(&self) -> &ResponseBody {
        self.http.body()
    }
}

impl Debug for Response {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Response")
            .field("version", &self.version())
            .field("status", &self.status())
            .field("headers", self.headers())
            .field("cookies", &self.cookies)
            .field("body", self.http.body())
            .finish()
    }
}

impl From<Response> for http::Response<ResponseBody> {
    #[inline]
    fn from(response: Response) -> Self {
        response.http
    }
}

impl From<http::Response<ResponseBody>> for Response {
    #[inline]
    fn from(http: http::Response<ResponseBody>) -> Self {
        Self {
            cookies: CookieJar::new(),
            http,
        }
    }
}
