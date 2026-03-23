use bytes::Bytes;
use futures_core::Stream;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING};
use http::{HeaderName, HeaderValue, StatusCode, Version};
use http_body::Frame;
use http_body_util::StreamBody;
use serde::Serialize;

use super::Response;
use super::body::ResponseBody;
use crate::error::{BoxError, Error};

/// Define how a type finalizes a [`ResponseBuilder`].
///
/// ```
/// use via::response::{Finalize, Response};
/// use via::{Next, Request};
///
/// async fn echo(request: Request, _: Next) -> via::Result {
///     request.finalize(Response::build().header("X-Powered-By", "Via"))
/// }
/// ```
///
pub trait Finalize {
    fn finalize(self, response: ResponseBuilder) -> Result<Response, Error>;
}

#[derive(Debug, Default)]
pub struct ResponseBuilder {
    response: http::response::Builder,
}

impl ResponseBuilder {
    #[inline]
    pub fn status<T>(mut self, status: T) -> Self
    where
        StatusCode: TryFrom<T>,
        <StatusCode as TryFrom<T>>::Error: Into<http::Error>,
    {
        self.response = self.response.status(status);
        self
    }

    #[inline]
    pub fn version(mut self, version: Version) -> Self {
        self.response = self.response.version(version);
        self
    }

    #[inline]
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.response = self.response.header(key, value);
        self
    }

    #[inline]
    pub fn extension<T>(mut self, extension: T) -> Self
    where
        T: Clone + Send + Sync + 'static,
    {
        self.response = self.response.extension(extension);
        self
    }

    #[inline]
    pub fn body(self, body: ResponseBody) -> Result<Response, Error> {
        Ok(self.response.body(body)?.into())
    }

    #[inline]
    pub fn json(self, body: &impl Serialize) -> Result<Response, Error> {
        let body = serde_json::to_vec(body).map_err(Error::ser_json)?;

        self.header(CONTENT_LENGTH, body.len())
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(ResponseBody::from(body))
    }

    #[inline]
    pub fn html(self, body: impl Into<String>) -> Result<Response, Error> {
        let body = body.into();

        self.header(CONTENT_LENGTH, body.len())
            .header(CONTENT_TYPE, "text/html; charset=utf-8")
            .body(ResponseBody::from(body))
    }

    #[inline]
    pub fn text(self, body: impl Into<String>) -> Result<Response, Error> {
        let body = body.into();

        self.header(CONTENT_LENGTH, body.len())
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(ResponseBody::from(body))
    }

    /// Convert self into a [Response] with an empty payload.
    ///
    #[inline]
    pub fn finish(self) -> Result<Response, Error> {
        self.body(ResponseBody::default())
    }
}

impl<T> Finalize for T
where
    T: Stream<Item = Result<Frame<Bytes>, BoxError>> + Send + Sync + 'static,
{
    #[inline]
    fn finalize(self, builder: ResponseBuilder) -> Result<Response, Error> {
        builder
            .header(TRANSFER_ENCODING, "chunked")
            .body(ResponseBody::boxed(StreamBody::new(self)))
    }
}
