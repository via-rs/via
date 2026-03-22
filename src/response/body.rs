use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use http_body_util::combinators::BoxBody;
use http_body_util::{Either, Full};
use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::error::BoxError;

pub struct ResponseBody {
    kind: Either<Full<Bytes>, BoxBody<Bytes, BoxError>>,
}

impl ResponseBody {
    #[inline]
    pub fn new(buf: Bytes) -> Self {
        Self {
            kind: Either::Left(Full::new(buf)),
        }
    }

    pub fn boxed<T>(body: T) -> Self
    where
        T: Body<Data = Bytes, Error = BoxError> + Send + Sync + 'static,
    {
        Self {
            kind: Either::Right(BoxBody::new(body)),
        }
    }
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
        Self::new(Default::default())
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

impl From<Bytes> for ResponseBody {
    #[inline]
    fn from(buf: Bytes) -> Self {
        Self::new(buf)
    }
}

impl From<String> for ResponseBody {
    #[inline]
    fn from(data: String) -> Self {
        Self::new(Bytes::from(data.into_bytes()))
    }
}

impl From<&'_ str> for ResponseBody {
    #[inline]
    fn from(data: &str) -> Self {
        Self::new(Bytes::copy_from_slice(data.as_bytes()))
    }
}

impl From<Vec<u8>> for ResponseBody {
    #[inline]
    fn from(data: Vec<u8>) -> Self {
        Self::new(Bytes::from(data))
    }
}

impl From<&'_ [u8]> for ResponseBody {
    #[inline]
    fn from(slice: &'_ [u8]) -> Self {
        Self::new(Bytes::copy_from_slice(slice))
    }
}
