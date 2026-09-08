use bytes::Bytes;
use http_body::Body;
use std::fmt::{self, Display, Formatter};
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::task::coop;

use super::Sender;
use crate::error::BoxError;

/// The concrete error for when `src` returns consecutively returns pending.
#[derive(Debug)]
struct SrcNotResponding;

pub struct PipeTask<T> {
    pipe: Pin<Box<Pipe<T>>>,
}

struct Pipe<T> {
    pending: bool,
    src: T,
    dest: Sender,
}

impl<T> PipeTask<T> {
    #[inline]
    pub fn new(src: T, dest: Sender) -> Self {
        Self {
            pipe: Box::pin(Pipe {
                pending: false,
                src,
                dest,
            }),
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

impl<T> Future for Pipe<T>
where
    T: Body<Data = Bytes, Error = BoxError> + Send,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        // Safety:
        //
        // `Self` only occurs as a field of `PipeTask` as `Pin<Box<Self>>`. The
        // visibility rules in this module uphold this invariant.
        //
        // Data that the pinning requirements of this implementation depend
        // upon do not move out of `self`. The only field that is mutated
        // directly is the `pending` counter byte.
        let this = unsafe { self.get_unchecked_mut() };

        loop {
            // Safety:
            //
            // `Pin<&mut T>` is used once per iteration to poll `src` for the
            // next frame. Typically, this loop completes one full iteration.
            //
            // `src` is guaranteed a stable memory address because `Self` only
            // occurs as a field of `PipeTask` as `Pin<Box<Self>>`. The `src`
            // field is never modified once it is initialized.
            //
            // Since `src` is a generic implementation of `Body`, we trust that
            // this implementation does not violate the pinning requirements
            // of `PipeTask`. For example, replacing the value of
            // `Pin<&mut Self>` with unsafe code.
            let src = unsafe { Pin::new_unchecked(&mut this.src) };

            // Fairness is enforced where backpressure accumulates.
            let coop = ready!(coop::poll_proceed(context));

            // Poll `src` for the next frame when `dest` has capacity for it.
            let poll_frame = match this.dest.poll_ready(context) {
                Poll::Ready(Ok(_)) => {
                    src.poll_frame(context) // capacity available
                }
                Poll::Pending => {
                    // The responsiveness of `dest` is outside of our control.
                    return Poll::Pending;
                }
                Poll::Ready(Err(_)) => {
                    log!(warn(pipe = 0), "connection closed.");
                    return Poll::Ready(());
                }
            };

            match poll_frame {
                Poll::Ready(Some(Ok(frame))) => {
                    // We have exclusive access to `dest` and we just confirmed
                    // readiness. If an error occurs, the connection closed.
                    if this.dest.send_frame(frame).is_err() {
                        log!(warn(pipe = 0), "connection closed.");
                        return Poll::Ready(());
                    }

                    // Progress is made when a `frame` from `src` is accepted by `dest`.
                    coop.made_progress();

                    // Reset the counter when progress is made.
                    this.pending = false;
                }
                Poll::Ready(None) => {
                    return Poll::Ready(()); // Exhausted
                }
                Poll::Pending => {
                    if this.pending {
                        let error = Box::new(SrcNotResponding);

                        if let Err(error) = this.dest.send_error(error) {
                            log!(error(pipe = 0), "{}", error);
                        } else {
                            this.dest.close_channel();
                        }

                        return Poll::Ready(());
                    } else {
                        this.pending = true;
                        return Poll::Pending;
                    }
                }
                Poll::Ready(Some(Err(error))) => {
                    // The connection closed, preventing the error from
                    // propagating. Log the error in debug builds.
                    if let Err(error) = this.dest.send_error(error) {
                        log!(error(pipe = 0), "{}", error);
                    }

                    return Poll::Ready(());
                }
            }
        }
    }
}

impl std::error::Error for SrcNotResponding {}
impl Display for SrcNotResponding {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "pipe task `src` is not responding.")
    }
}
