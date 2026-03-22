mod body;
mod builder;
mod redirect;

#[cfg(feature = "file")]
mod file;

pub use body::{Json, ResponseBody};
pub use builder::{Finalize, ResponseBuilder};
use delegate::delegate;
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

    delegate! {
        to self.inner() {
            pub fn status(&self) -> StatusCode;
        }

        to self.inner_mut() {
            pub fn status_mut(&mut self) -> &mut StatusCode;
        }

        to self.inner() {
            pub fn version(&self) -> Version;
            pub fn headers(&self) -> &HeaderMap;
        }

        to self.inner_mut() {
            pub fn headers_mut(&mut self) -> &mut HeaderMap;
        }
    }

    /// Returns a reference to the response cookies.
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }

    /// Returns a mutable reference to the response cookies.
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }

    delegate! {
        to self.inner() {
            pub fn extensions(&self) -> &Extensions;
        }

        to self.inner_mut() {
            pub fn extensions_mut(&mut self) -> &mut Extensions;
        }
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
