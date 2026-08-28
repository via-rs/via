use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use http_body_util::channel::{Channel, Sender};
use http_body_util::{Either, Full};
use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::task::coop;

use crate::error::BoxError;

pub struct ResponseBody {
    body: Either<ChannelBody, ReadyBody>,
}

struct ChannelBody {
    body: Channel<Bytes, BoxError>,
}

struct ReadyBody {
    body: Full<Bytes>,
}

struct PipeTask<T> {
    pipe: Pin<Box<Pipe<T>>>,
}

struct Pipe<T> {
    source: T,
    queue: Option<Frame<Bytes>>,
    dest: Option<Sender<Bytes, BoxError>>,
}

impl<T> PipeTask<T> {
    fn new(source: T, dest: Sender<Bytes, BoxError>) -> Self {
        Self {
            pipe: Box::pin(Pipe {
                source,
                queue: None,
                dest: Some(dest),
            }),
        }
    }
}

impl<T> Future for PipeTask<T>
where
    T: Body<Data = Bytes, Error = BoxError> + Send + Unpin,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        self.pipe.as_mut().poll(context)
    }
}

impl<T> Future for Pipe<T>
where
    T: Body<Data = Bytes, Error = BoxError> + Send + Unpin,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let coop = ready!(coop::poll_proceed(context));

        if let Some(frame) = self.queue.take() {
            // If sending the message fails, rx was dropped or the connection
            // stalled. The safest thing we can do is consider it a timeout.
            if let Some(tx) = self.dest.take_if(|tx| tx.try_send(frame).is_err()) {
                tx.abort("body write timeout".to_owned().into());
                return Poll::Ready(());
            }
        }

        match Pin::new(&mut self.source).poll_frame(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(()),
            Poll::Ready(Some(Ok(frame))) => {
                coop.made_progress();

                if let Some(tx) = self.dest.as_mut() {
                    self.queue = tx.try_send(frame).err();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            }
            Poll::Ready(Some(Err(error))) => {
                if let Some(tx) = self.dest.take() {
                    tx.abort(error);
                }

                Poll::Ready(())
            }
        }
    }
}

impl ResponseBody {
    #[inline]
    pub fn new(buf: Bytes) -> Self {
        Self {
            body: Either::Right(ReadyBody {
                body: Full::new(buf),
            }),
        }
    }

    #[inline]
    pub fn spawn<T>(source: T) -> Self
    where
        T: Body<Data = Bytes, Error = BoxError> + Send + Unpin + 'static,
    {
        let (tx, rx) = Channel::new(1);

        tokio::spawn(PipeTask::new(source, tx));

        Self {
            body: Either::Left(ChannelBody { body: rx }),
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
        Pin::new(&mut self.body).poll_frame(context)
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}

impl Body for ChannelBody {
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

impl Body for ReadyBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match Pin::new(&mut self.body).poll_frame(context) {
            Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,

            // The error type of `self.body` is `Infallible`.
            // At a minimum, this arm is a cold path. Ideally it is eliminated.
            Poll::Ready(Some(Err(_))) => unreachable!(),
        }
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
