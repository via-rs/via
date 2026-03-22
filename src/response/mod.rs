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

use cookie::CookieJar;
use http::{Extensions, HeaderMap, StatusCode, Version};
use std::fmt::{self, Debug, Formatter};

pub struct Response {
    inner: http::Response<ResponseBody>,
    cookies: CookieJar,
}

impl Response {
    #[inline]
    pub fn new(body: ResponseBody) -> Self {
        Self {
            inner: http::Response::new(body),
            cookies: CookieJar::new(),
        }
    }

    #[inline]
    pub fn build() -> ResponseBuilder {
        Default::default()
    }

    pub fn status(&self) -> StatusCode {
        self.inner().status()
    }

    pub fn status_mut(&mut self) -> &mut StatusCode {
        self.inner_mut().status_mut()
    }

    pub fn version(&self) -> Version {
        self.inner().version()
    }

    pub fn headers(&self) -> &HeaderMap {
        self.inner().headers()
    }

    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        self.inner_mut().headers_mut()
    }

    /// Returns a reference to the response cookies.
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }

    /// Returns a mutable reference to the response cookies.
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }

    pub fn extensions(&self) -> &Extensions {
        self.inner().extensions()
    }

    pub fn extensions_mut(&mut self) -> &mut Extensions {
        self.inner_mut().extensions_mut()
    }

    #[inline]
    fn inner(&self) -> &http::Response<ResponseBody> {
        &self.inner
    }

    #[inline]
    fn inner_mut(&mut self) -> &mut http::Response<ResponseBody> {
        &mut self.inner
    }
}

impl Debug for Response {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status())
            .field("version", &self.version())
            .field("headers", self.headers())
            .field("cookies", &self.cookies)
            .field("body", self.inner.body())
            .finish()
    }
}

impl From<Response> for http::Response<ResponseBody> {
    #[inline]
    fn from(response: Response) -> Self {
        response.inner
    }
}

impl From<http::Response<ResponseBody>> for Response {
    #[inline]
    fn from(inner: http::Response<ResponseBody>) -> Self {
        Self {
            inner,
            cookies: CookieJar::new(),
        }
    }
}
