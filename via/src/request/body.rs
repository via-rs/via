use bytes::{Buf, Bytes};
use http::HeaderMap;
use http_body::{Body, Frame, SizeHint};
use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, ready};
use std::time::Duration;

use crate::error::Error;

#[cfg(feature = "test-util")]
use crate::test::TestBody;

/// The data and trailers of a request body.
///
pub struct Aggregate {
    payload: RequestPayload,
    _unsend: PhantomData<Rc<()>>,
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Coalesce {
    body: RequestBody,
}

pub struct RequestBody {
    remaining: usize,

    #[cfg(feature = "test-util")]
    body: TestBody,

    #[cfg(not(feature = "test-util"))]
    body: hyper::body::Incoming,

    frames: Option<Vec<Bytes>>,
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct WithTrailers {
    body: RequestBody,
    trailers: Option<HeaderMap>,
}

pub(super) struct RequestPayload {
    frames: Vec<Bytes>,
    trailers: Option<HeaderMap>,
}

macro_rules! impl_timeout_after {
    ($ty:ident) => {
        impl $ty {
            /// Respond with a `408` Request Timeout error if the future is not
            /// ready within the specified duration.
            pub async fn timeout_after(self, duration: Duration) -> Result<Aggregate, Error> {
                let Ok(result) = tokio::time::timeout(duration, self).await else {
                    crate::deny!(408, "request timeout.");
                };

                result
            }

            /// Respond with a `408` Request Timeout error if the future is not
            /// ready within the specified timeout in seconds.
            pub fn timeout_after_secs(
                self,
                seconds: u64,
            ) -> impl Future<Output = Result<Aggregate, Error>> + Send {
                self.timeout_after(Duration::from_secs(seconds))
            }
        }
    };
}

fn already_read() -> Error {
    crate::err!(500, "a request body can only be read once.")
}

fn unknown_frame_type() -> Error {
    crate::err!(400, "unknown frame type encountered in request.")
}

impl Aggregate {
    pub fn trailers(&self) -> Option<&HeaderMap> {
        self.payload.trailers.as_ref()
    }

    pub fn is_empty(&self) -> bool {
        self.len().is_some_and(|len| len == 0)
    }

    #[inline]
    pub fn len(&self) -> Option<usize> {
        self.payload()
            .frames()
            .iter()
            .map(Buf::remaining)
            .try_fold(0usize, |len, remaining| len.checked_add(remaining))
    }
}

impl Aggregate {
    #[inline]
    pub(super) fn payload(&self) -> &RequestPayload {
        &self.payload
    }

    #[inline]
    pub(super) fn payload_mut(&mut self) -> &mut RequestPayload {
        &mut self.payload
    }
}

impl Coalesce {
    #[inline]
    pub fn with_trailers(self) -> WithTrailers {
        WithTrailers {
            body: self.body,
            trailers: None,
        }
    }
}

impl_timeout_after!(Coalesce);

impl Coalesce {
    #[inline]
    pub(super) fn new(body: RequestBody) -> Self {
        Self { body }
    }
}

impl Future for Coalesce {
    type Output = Result<Aggregate, Error>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        while let Some(frame) = ready!(Pin::new(&mut self.body).poll_frame(context)?) {
            let frames = self.body.frames_mut()?;
            if let Ok(data) = frame.into_data() {
                frames.push(data);
            }
        }

        Poll::Ready(self.body.finish(None))
    }
}

impl RequestBody {
    /// Aggregate the frames of the request body into a contiguous block of
    /// memory.
    #[inline]
    pub fn coalesce(self) -> Coalesce {
        Coalesce { body: self }
    }
}

impl RequestBody {
    #[cfg(feature = "test-util")]
    pub(crate) fn new(remaining: usize, body: impl Into<TestBody>, frames: Vec<Bytes>) -> Self {
        Self {
            remaining,
            body: body.into(),
            frames: Some(frames),
        }
    }

    #[cfg(not(feature = "test-util"))]
    pub(crate) fn new(remaining: usize, body: hyper::body::Incoming, frames: Vec<Bytes>) -> Self {
        Self {
            remaining,
            body,
            frames: Some(frames),
        }
    }

    fn finish(&mut self, trailers: Option<HeaderMap>) -> Result<Aggregate, Error> {
        let frames = self.frames.take().ok_or_else(already_read)?;

        Ok(Aggregate {
            payload: RequestPayload { frames, trailers },
            _unsend: PhantomData,
        })
    }

    fn frames_mut(&mut self) -> Result<&mut Vec<Bytes>, Error> {
        self.frames.as_mut().ok_or_else(already_read)
    }

    fn has_capacity(&self) -> bool {
        self.body.size_hint().exact().is_none_or(|upper| {
            u64::try_from(self.remaining).is_ok_and(|remaining| remaining >= upper)
        })
    }
}

impl Body for RequestBody {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        if self.remaining == 0 || !self.has_capacity() {
            return Poll::Ready(Some(Err(Error::payload_too_large())));
        }

        let Some(frame) = ready!(Pin::new(&mut self.body).poll_frame(context)?) else {
            return Poll::Ready(None);
        };

        if let Some(data) = frame.data_ref() {
            let Some(remaining) = self.remaining.checked_sub(data.remaining()) else {
                self.remaining = 0;
                return Poll::Ready(Some(Err(Error::payload_too_large())));
            };

            self.remaining = remaining;
        }

        Poll::Ready(Some(Ok(frame)))
    }

    fn is_end_stream(&self) -> bool {
        self.remaining == 0 || !self.has_capacity() || self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        let Ok(remaining) = u64::try_from(self.remaining) else {
            let mut hint = SizeHint::new();

            hint.set_lower(self.body.size_hint().lower());

            #[cfg(debug_assertions)]
            crate::util::once!(|| {
                print!("warn(via): a lossy size hint must be used for RequestBody. ");
                println!("usize::MAX exceeds u64::MAX on this platform.");
            });

            return hint;
        };

        let mut hint = self.body.size_hint();

        if remaining < hint.lower() {
            hint.set_exact(remaining);
        } else {
            let upper = hint.upper().map_or(remaining, |upper| upper.min(remaining));
            hint.set_upper(upper);
        }

        hint
    }
}

impl Debug for RequestBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestBody").finish()
    }
}

impl RequestPayload {
    #[inline(always)]
    pub(super) fn frames(&self) -> &[Bytes] {
        &self.frames
    }

    #[inline(always)]
    pub(super) fn frames_mut(&mut self) -> &mut [Bytes] {
        &mut self.frames
    }

    #[inline(always)]
    pub(super) fn unary_mut(&mut self) -> Option<&mut Bytes> {
        if self.frames.len() == 1 {
            Some(&mut self.frames[0])
        } else {
            None
        }
    }
}

impl_timeout_after!(WithTrailers);

impl Future for WithTrailers {
    type Output = Result<Aggregate, Error>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        while let Some(frame) = ready!(Pin::new(&mut self.body).poll_frame(context)?) {
            match frame.into_data() {
                Ok(data) => {
                    self.body.frames_mut()?.push(data);
                }
                Err(frame) => {
                    let trailers = frame.into_trailers().map_err(|_| unknown_frame_type())?;
                    if let Some(existing) = self.trailers.as_mut() {
                        existing.extend(trailers);
                    } else {
                        self.trailers = Some(trailers);
                    }
                }
            }
        }

        let trailers = self.trailers.take();
        Poll::Ready(self.body.finish(trailers))
    }
}
