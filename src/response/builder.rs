use bytes::Bytes;
use futures_core::Stream;
use http::{HeaderName, HeaderValue, StatusCode, Version, header};
use http_body::Frame;
use http_body_util::StreamBody;
use http_body_util::combinators::BoxBody;
use serde::Serialize;

use super::{Json, Response, ResponseBody};
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
    pub fn status<T>(self, status: T) -> Self
    where
        StatusCode: TryFrom<T>,
        <StatusCode as TryFrom<T>>::Error: Into<http::Error>,
    {
        Self {
            response: self.response.status(status),
        }
    }

    #[inline]
    pub fn version(self, version: Version) -> Self {
        Self {
            response: self.response.version(version),
        }
    }

    #[inline]
    pub fn header<K, V>(self, key: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        Self {
            response: self.response.header(key, value),
        }
    }

    #[inline]
    pub fn extension<T>(self, extension: T) -> Self
    where
        T: Clone + Send + Sync + 'static,
    {
        Self {
            response: self.response.extension(extension),
        }
    }

    #[inline]
    pub fn body(self, body: ResponseBody) -> Result<Response, Error> {
        Ok(self.response.body(body)?.into())
    }

    #[inline]
    pub fn json<T>(self, body: &T) -> Result<Response, Error>
    where
        T: Serialize,
    {
        #[derive(Serialize)]
        struct Tagged<'a, D> {
            data: &'a D,
        }

        Json(&Tagged { data: body }).finalize(self)
    }

    #[inline]
    pub fn html(self, body: impl Into<String>) -> Result<Response, Error> {
        let string = body.into();

        self.header(header::CONTENT_LENGTH, string.len())
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(string.into())
    }

    #[inline]
    pub fn text(self, body: impl Into<String>) -> Result<Response, Error> {
        let string = body.into();

        self.header(header::CONTENT_LENGTH, string.len())
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(string.into())
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
            .header(header::TRANSFER_ENCODING, "chunked")
            .body(BoxBody::new(StreamBody::new(self)).into())
    }
}
