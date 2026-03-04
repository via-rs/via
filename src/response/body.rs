use bytes::Bytes;
use http::header;
use http_body::{Body, Frame, SizeHint};
use http_body_util::combinators::BoxBody;
use http_body_util::{Either, Full};
use serde::Serialize;
use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{Finalize, Response, ResponseBuilder};
use crate::error::{BoxError, Error};

/// Serialize the contained type as an untagged JSON response.
///
/// # Example
/// ```
/// use serde::Serialize;
/// use via::response::{Finalize, Json, Response};
///
/// #[derive(Serialize)]
/// struct Cat {
///     name: String,
/// }
///
/// impl Cat {
///     fn new(name: String) -> Self {
///         Self { name }
///     }
/// }
///
/// let ciro = Cat::new("Ciro".to_owned());
/// let response = Response::build().json(&ciro).unwrap();
/// // body: { "data": { "name": "Ciro" } }
///
/// let ciro = Cat::new("Ciro".to_owned());
/// let response = Json(&ciro).finalize(Response::build()).unwrap();
/// // body: { "name": "Ciro" }
/// ```
///
pub struct Json<'a, T>(pub &'a T);

pub struct ResponseBody {
    kind: Either<Full<Bytes>, BoxBody<Bytes, BoxError>>,
}

impl Body for ResponseBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.kind).poll_frame(context)
    }

    fn is_end_stream(&self) -> bool {
        self.kind.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.kind.size_hint()
    }
}

impl Debug for ResponseBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        #[derive(Debug)]
        struct BoxBody;

        #[derive(Debug)]
        struct Full;

        let kind = match &self.kind {
            Either::Left(_) => Either::Left(Full),
            Either::Right(_) => Either::Right(BoxBody),
        };

        f.debug_struct("ResponseBody").field("kind", &kind).finish()
    }
}

impl Default for ResponseBody {
    #[inline]
    fn default() -> Self {
        Self {
            kind: Either::Left(Default::default()),
        }
    }
}

impl From<Bytes> for ResponseBody {
    #[inline]
    fn from(buf: Bytes) -> Self {
        Self {
            kind: Either::Left(Full::new(buf)),
        }
    }
}

impl From<BoxBody<Bytes, BoxError>> for ResponseBody {
    #[inline]
    fn from(body: BoxBody<Bytes, BoxError>) -> Self {
        Self {
            kind: Either::Right(body),
        }
    }
}

impl From<String> for ResponseBody {
    #[inline]
    fn from(data: String) -> Self {
        Self::from(data.into_bytes())
    }
}

impl From<&'_ str> for ResponseBody {
    #[inline]
    fn from(data: &str) -> Self {
        Self::from(data.as_bytes())
    }
}

impl From<Vec<u8>> for ResponseBody {
    #[inline]
    fn from(data: Vec<u8>) -> Self {
        Self::from(Bytes::from(data))
    }
}

impl From<&'_ [u8]> for ResponseBody {
    #[inline]
    fn from(slice: &'_ [u8]) -> Self {
        Self::from(Bytes::copy_from_slice(slice))
    }
}

impl<T: Serialize> Finalize for Json<'_, T> {
    #[inline]
    fn finalize(self, response: ResponseBuilder) -> Result<Response, Error> {
        let body = serde_json::to_string(self.0)?;

        response
            .header(header::CONTENT_LENGTH, body.len())
            .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .body(body.into())
    }
}
