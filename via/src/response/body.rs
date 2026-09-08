use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use http_body_util::{Full, combinators::BoxBody};
use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::task;

use super::channel::{ChannelBody, PipeTask, Sender};
use crate::error::BoxError;

pub struct ResponseBody {
    body: BoxBody<Bytes, BoxError>,
}

struct ReadyBody {
    body: Full<Bytes>,
}

impl ReadyBody {
    #[inline]
    fn new(buf: Bytes) -> Self {
        Self {
            body: Full::new(buf),
        }
    }
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
        Self::boxed(ReadyBody::new(buf))
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
    pub fn channel(f: impl FnOnce(Sender)) -> Self {
        let (tx, body) = ChannelBody::new();
        let body = Self::boxed(body);

        f(tx);

        body
    }

    pub fn once(buf: Bytes) -> Self {
        Self::spawn(ReadyBody::new(buf))
    }

    pub fn spawn<T>(src: T) -> Self
    where
        T: Body<Data = Bytes, Error = BoxError> + Send + 'static,
    {
        Self::channel(|dest| {
            // Spawn a task to pipe the frames from `src` to `dest`.
            task::spawn(PipeTask::new(src, dest));
        })
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

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use futures_core::Stream;
    use http_body::{Body, Frame};
    use http_body_util::{BodyExt, StreamBody};
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::task::{Context, Poll};
    use tokio::task;

    use super::{PipeTask, ReadyBody, ResponseBody};
    use crate::error::BoxError;

    const GREETING: Bytes = Bytes::from_static(b"Hello, world!");

    /// An `impl Body` that is immediately `Poll::Ready` with an error.
    struct ErrorBody;

    /// An `impl Body` that always returns `Poll::Pending`.
    struct NeverBody;

    /// An `impl Stream` that splits `GREETING` into two frames.
    struct SplitGreeting {
        parts: Vec<Bytes>,
    }

    /// An `impl Body` that returns `Poll::Pending` before delegating to a
    /// `ReadyBody`.
    struct YieldThenBody {
        did_yield: bool,
        body: ReadyBody,
    }

    impl Body for ErrorBody {
        type Data = Bytes;
        type Error = BoxError;

        fn poll_frame(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(Some(Err("an error occurred.".into())))
        }
    }

    impl Body for NeverBody {
        type Data = Bytes;
        type Error = BoxError;

        fn poll_frame(
            self: Pin<&mut Self>,
            context: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            context.waker().wake_by_ref();
            Poll::Pending
        }
    }

    impl SplitGreeting {
        fn new() -> Self {
            let mut head = GREETING;
            let tail = head.split_off(head.iter().position(|byte| b' ' == *byte).unwrap());

            Self {
                parts: vec![tail, head],
            }
        }
    }

    impl Stream for SplitGreeting {
        type Item = Result<Frame<Bytes>, BoxError>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            let next = self.parts.pop().map(|next| Ok(Frame::data(next)));
            context.waker().wake_by_ref();
            Poll::Ready(next)
        }
    }

    impl YieldThenBody {
        fn new(body: ReadyBody) -> Self {
            Self {
                did_yield: false,
                body,
            }
        }
    }

    impl Body for YieldThenBody {
        type Data = Bytes;
        type Error = BoxError;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            context: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            if self.did_yield {
                Pin::new(&mut self.body).poll_frame(context)
            } else {
                context.waker().wake_by_ref();
                self.did_yield = true;
                Poll::Pending
            }
        }
    }

    #[tokio::test]
    async fn once_yields_exactly_one_frame() {
        let mut body = ResponseBody::once(GREETING);
        let first = body
            .frame()
            .await
            .expect("`body` is not empty.")
            .expect("`body` is infallible unless cancelled.")
            .into_data()
            .expect("`body` only produces data frames.");

        assert_eq!(
            first, GREETING,
            "the bytes in `echo` are equal to the bytes of `greeting`."
        );

        assert!(
            body.frame().await.is_none(),
            "subsequent attempts to read the next frame are `None`."
        );
    }

    #[tokio::test]
    async fn once_produces_the_same_result_as_new() {
        let new = ResponseBody::new(GREETING)
            .collect()
            .await
            .expect("`new` is infallible unless cancelled.");

        let once = ResponseBody::once(GREETING)
            .collect()
            .await
            .expect("`once` is infallible unless cancelled.");

        assert_eq!(
            new.to_bytes(),
            once.to_bytes(),
            "`new` and `once` produce the same result.",
        );
    }

    #[tokio::test]
    async fn spawn_produces_the_same_result_as_boxed() {
        let boxed = ResponseBody::boxed(StreamBody::new(SplitGreeting::new()))
            .collect()
            .await
            .expect("`SplitGreeting` is infallible.");

        // Eagerly convert `boxed` into a contiguous `Bytes`.
        let expect = boxed.to_bytes();

        let spawn = ResponseBody::spawn(StreamBody::new(SplitGreeting::new()))
            .collect()
            .await
            .expect("`SplitGreeting` is infallible.");

        assert_eq!(
            expect, GREETING,
            "`SplitGreeting` yields `GREETING` in two parts.",
        );

        assert_eq!(
            expect,
            spawn.to_bytes(),
            "when given the same stream, `boxed` and `spawn` produce the same result.",
        );
    }

    #[tokio::test]
    async fn pipe_task_exits_when_dest_is_dropped() {
        let handle = Arc::new(());
        let body = ResponseBody::channel(|dest| {
            let handle = Arc::clone(&handle);
            let pipe = PipeTask::new(NeverBody, dest);

            task::spawn(async move {
                let _handle = handle;
                pipe.await
            });
        });

        assert_eq!(
            2,
            Arc::strong_count(&handle),
            "a clone of `polls` should move into the pipe task.",
        );

        // Simulate a closed connection by dropping `body`.
        drop(body);

        // Yield to the runtime so the pipe task can be polled.
        task::yield_now().await;

        assert_eq!(
            1,
            Arc::strong_count(&handle),
            "the pipe task exits when `dest` is dropped."
        );
    }

    #[tokio::test]
    async fn pipe_task_exits_when_src_errors() {
        let handle = Arc::new(());
        let body = ResponseBody::channel(|dest| {
            let handle = Arc::clone(&handle);
            let pipe = PipeTask::new(ErrorBody, dest);

            task::spawn(async move {
                let _handle = handle;
                pipe.await
            });
        });

        assert_eq!(
            2,
            Arc::strong_count(&handle),
            "a clone of `handle` should move into the pipe task.",
        );

        body.collect().await.expect_err("`src` is an `ErrorBody`.");

        assert_eq!(
            1,
            Arc::strong_count(&handle),
            "the pipe task exits when `src` errors."
        );
    }

    #[tokio::test]
    async fn pipe_task_exits_when_src_is_exhausted() {
        let handle = Arc::new(());
        let body = ResponseBody::channel(|dest| {
            let src = YieldThenBody::new(ReadyBody::new(GREETING));
            let pipe = PipeTask::new(src, dest);
            let handle = Arc::clone(&handle);

            task::spawn(async move {
                let _handle = handle;
                pipe.await
            });
        });

        assert_eq!(
            2,
            Arc::strong_count(&handle),
            "a clone of `handle` should move into the pipe task.",
        );

        let collected = body
            .collect()
            .await
            .expect("`src` is infallible unless cancelled.");

        assert_eq!(
            GREETING,
            collected.to_bytes(),
            "`pipe` moves the bytes in `src` to `dest`.",
        );

        assert_eq!(
            1,
            Arc::strong_count(&handle),
            "the pipe task exits when `src` is exhausted."
        );
    }

    #[tokio::test]
    async fn pipe_task_exits_when_src_is_unresponsive() {
        let polls = Arc::new(AtomicU32::new(0));
        let body = ResponseBody::channel(|dest| {
            let mut pipe = PipeTask::new(NeverBody, dest);
            let polls = Arc::clone(&polls);

            task::spawn(async move {
                let future = std::future::poll_fn(|context| {
                    // Poll `src` for the next frame.
                    let poll = Pin::new(&mut pipe).poll(context);

                    // Increment the `polls` counter.
                    polls.fetch_add(1, Ordering::SeqCst);

                    poll
                });

                future.await
            });
        });

        assert_eq!(
            2,
            Arc::strong_count(&polls),
            "a clone of `polls` should move into the pipe task.",
        );

        let _ = body
            .collect()
            .await
            .expect_err("an unresponsive `src` propagates an error to `dest`.");

        assert_eq!(
            1,
            Arc::strong_count(&polls),
            "the pipe task exits when `src` becomes unresponsive."
        );

        assert_eq!(
            2,
            polls.load(Ordering::SeqCst),
            "`src` is considered unresponsive after the second `Poll::Pending`."
        );
    }
}
