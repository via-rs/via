use bytes::Bytes;
use http_body::Body;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::task::coop;

use super::Sender;
use crate::error::BoxError;

pub struct PipeTask<T> {
    pipe: Pin<Box<Pipe<T>>>,
}

struct Pipe<T> {
    src: T,
    dest: Sender,
}

impl<T> PipeTask<T> {
    #[inline]
    pub fn new(src: T, dest: Sender) -> Self {
        Self {
            pipe: Box::pin(Pipe { src, dest }),
        }
    }
}

impl<T> Future for PipeTask<T>
where
    T: Body<Data = Bytes, Error = BoxError> + Send,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        self.pipe.as_mut().poll(context)
    }
}

impl<T> Pipe<T> {
    #[inline(always)]
    fn project(self: Pin<&mut Self>) -> (Pin<&mut T>, Pin<&mut Sender>) {
        // Safety:
        //
        // `Pipe` can only be constructed as `Pin<Box<Pipe>>` guaranteeing a
        // stable memory address.
        //
        // Data that the pinning invariants of `Pipe` depend upon do not move
        // from any `Pin<&mut _>` created from `this`.
        let this = unsafe { self.get_unchecked_mut() };

        // Safety:
        //
        // `Pin<&mut T>` is used once to poll the producer. `src` never moves
        // out of `self` in the process. We trust that `T` is well behaved with
        // regards to it's own pinning invariants.
        let src = unsafe { Pin::new_unchecked(&mut this.src) };

        // `Sender` is `Unpin` and does not require `unsafe` for projection.
        let dest = Pin::new(&mut this.dest);

        (src, dest)
    }
}

impl<T> Future for Pipe<T>
where
    T: Body<Data = Bytes, Error = BoxError> + Send,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            // Ideally, this returns pending and wake gets registered without
            // polling `dest` for readiness.
            let coop = ready!(coop::poll_proceed(context));
            let (src, mut dest) = self.as_mut().project();

            // We know the channel is not full because it would return pending.
            if ready!(dest.poll_ready(context)).is_err() {
                log!(warn(pipe = 0), "readiness error. connection closed.");
                return Poll::Ready(());
            }

            match ready!(src.poll_frame(context)) {
                Some(Ok(frame)) => {
                    // We have exclusive access to `dest` and we just confirmed
                    // readiness. If an error occurs, the connection closed.
                    if dest.send_frame(frame).is_err() {
                        log!(warn(pipe = 0), "send error. connection closed.");
                        return Poll::Ready(());
                    }

                    coop.made_progress();
                }
                Some(Err(error)) => {
                    // The connection closed, preventing the error from
                    // propagating. Log the error in debug builds.
                    if let Err(error) = dest.send_error(error) {
                        log!(error(pipe = 0), "{}", error);
                    }

                    return Poll::Ready(());
                }
                None => {
                    return Poll::Ready(()); // Exhausted
                }
            }
        }
    }
}
