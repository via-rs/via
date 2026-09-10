use bytes::Bytes;
use http_body::Body;
use std::fmt::{self, Display, Formatter};
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::task::coop;

use super::Sender;
use crate::error::BoxError;

pub struct PipeTask<T> {
    pipe: Pin<Box<Pipe<T>>>,
}

/// The concrete error for when `src` returns consecutively returns pending.
#[derive(Debug)]
struct NotResponding;

struct Pipe<T> {
    pending: bool,
    src: T,
    dest: Sender,
}

/// Calls `Sender::send_error` with the provided `$error` and then closed the
/// channel. The readiness of `$dest` must be confirmed before this macro is
/// invoked.
macro_rules! send_error {
    ($dest:expr, $error:expr) => {
        if let Err(error) = Sender::send_error($dest, $error) {
            // Readiness is confirmed before `send_error` is called.
            std::hint::cold_path();

            log!(error(pipe = 0), "{}", error);
        }

        Sender::close_channel($dest);
    };
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
        // The mutable reference is used to project the `src` field that might
        // be an `impl Body + !Unpin`. This projection is used one time to poll
        // `src`.
        //
        // Subsequent iterations of this loop will return `Poll::Pending` and
        // register a wake when the authority accepts the next frame from `dest`.
        //
        // Since the `pending` flag and `dest` field are both `Unpin` and the
        // following loop only polls `src` once, the ideal implementation is
        // direct field access to create the projection to `src` at the last
        // minute. This avoids reborrowing `self` to project `src` once per
        // "iteration" (N+1 deref / reborrow) and effectively makes the `Pin`
        // wrapper a compile-time guard rail against accidental moves. Making
        // the code easier to audit against illegal aliasing.
        let this = unsafe { self.get_unchecked_mut() };

        loop {
            // Fairness is enforced where backpressure accumulates.
            let coop = ready!(coop::poll_proceed(context));

            // Poll `src` for the next frame when `dest` has capacity for it.
            if ready!(this.dest.poll_ready(context)).is_ok() {
                // Safety:
                //
                // `src` is guaranteed a stable memory address because `Self`
                // only occurs as a field of `PipeTask` as `Pin<Box<Self>>`.
                //
                // The `src` field is never modified after initialization.
                // As long as the implementation of `Body::poll_frame` does
                // not replace `self` with some other value, the address of
                // the source field remains stable and the value to which
                // it points to does not change.
                let src = unsafe { Pin::new_unchecked(&mut this.src) };

                match src.poll_frame(context) {
                    Poll::Ready(Some(Ok(frame))) => {
                        if this.dest.send_frame(frame).is_err() {
                            // We have exclusive access to `dest` and we polled
                            // its readiness.
                            //
                            // The channel disconnecting between the call to
                            // `poll_ready` and `send_frame` is practically
                            // impossible.
                            std::hint::cold_path();

                            log!(warn(pipe = 0), "connection closed.");

                            return Poll::Ready(());
                        }

                        // Define progress as a frame being accepted by `dest`.
                        coop.made_progress();

                        // Reset the `pending` flag when progress is made.
                        this.pending = false;
                    }
                    Poll::Ready(None) => {
                        return Poll::Ready(()); // `src` exhausted.
                    }
                    Poll::Pending => {
                        if this.pending {
                            let error = Box::new(NotResponding);
                            send_error!(&mut this.dest, error);
                            return Poll::Ready(());
                        } else {
                            this.pending = true;
                            return Poll::Pending;
                        }
                    }
                    Poll::Ready(Some(Err(error))) => {
                        send_error!(&mut this.dest, error);
                        return Poll::Ready(());
                    }
                }
            } else {
                log!(warn(pipe = 0), "connection closed.");
                return Poll::Ready(());
            }
        }
    }
}

impl std::error::Error for NotResponding {}

impl Display for NotResponding {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ResponseBody::spawn is not responding.")
    }
}
