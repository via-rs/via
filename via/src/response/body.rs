use bytes::Bytes;
use futures_core::Stream;
use http_body::{Body, Frame, SizeHint};
use http_body_util::{Full, combinators::BoxBody};
use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::task;

use super::channel::{ChannelBody, PipeTask};
use crate::error::BoxError;

pub struct ResponseBody {
    body: BoxBody<Bytes, BoxError>,
}

struct ReadyBody {
    body: Full<Bytes>,
}

struct StreamBody<T, E> {
    body: T,
    _err: PhantomData<E>,
}

impl Body for ReadyBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match ready!(Pin::new(&mut self.body).poll_frame(context)) {
            Some(Ok(frame)) => Poll::Ready(Some(Ok(frame))),
            None => Poll::Ready(None),

            // The error type of `self.body` is `Infallible`.
            // At a minimum, this arm is a cold path. Ideally it is eliminated.
            Some(Err(_)) => unreachable!(),
        }
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}

impl ResponseBody {
    #[inline]
    pub fn new(buf: Bytes) -> Self {
        Self::boxed(ReadyBody {
            body: Full::new(buf),
        })
    }

    #[inline]
    pub fn boxed<T>(body: T) -> Self
    where
        T: Body<Data = Bytes, Error = BoxError> + Send + Sync + 'static,
    {
        Self {
            body: BoxBody::new(body),
        }
    }

    #[inline]
    pub fn once(buf: Bytes) -> Self {
        Self::spawn(ReadyBody {
            body: Full::new(buf),
        })
    }

    #[inline]
    pub fn pipe<T, E>(src: T) -> Self
    where
        T: Stream<Item = Result<Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::spawn(StreamBody {
            body: src,
            _err: PhantomData,
        })
    }

    #[inline]
    pub fn spawn<T>(src: T) -> Self
    where
        T: Body<Data = Bytes, Error = BoxError> + Send + 'static,
    {
        let (dest, body) = ChannelBody::new();

        // Spawn a task to pipe the frames from `src` to `dest`.
        task::spawn(PipeTask::new(src, dest));

        // Return the receiver in a `BoxBody`.
        Self::boxed(body)
    }
}

impl Body for ResponseBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.body).poll_frame(context)
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}

impl Debug for ResponseBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseBody").finish()
    }
}

impl Default for ResponseBody {
    #[inline]
    fn default() -> Self {
        Self::new(Default::default())
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

impl<T, E> StreamBody<T, E> {
    #[inline(always)]
    fn project(self: Pin<&mut Self>) -> Pin<&mut T> {
        // Safety:
        //
        // The memory address of `self` is stable and pin-safe and body never
        // moves out of `self` from the returned `Pin<&mut T>`.
        unsafe { Pin::map_unchecked_mut(self, |this| &mut this.body) }
    }
}

impl<T, E> Body for StreamBody<T, E>
where
    T: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        context: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match ready!(self.project().poll_next(context)) {
            Some(Ok(buf)) => Poll::Ready(Some(Ok(Frame::data(buf)))),
            Some(Err(error)) => Poll::Ready(Some(Err(Box::new(error)))),
            None => Poll::Ready(None),
        }
    }
}
