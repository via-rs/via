use bytes::{Buf, Bytes};
use http::{HeaderMap, StatusCode};
use http_body::Body;
use http_body_util::{LengthLimitError, Limited};
use hyper::body::Incoming;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use smallvec::SmallVec;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{Ordering, compiler_fence};
use std::task::{Context, Poll, ready};
use std::{ptr, slice};

#[cfg(feature = "ws")]
use tungstenite::protocol::frame::Utf8Bytes;

use crate::error::{BoxError, Error};
use crate::raise;

mod sealed {
    /// Prevents external implementations of Payload. Allowing us to make
    /// assumptions about the data contained by implementations of Payload.
    pub trait Sealed {}

    impl Sealed for super::Aggregate {}

    #[cfg(feature = "ws")]
    impl Sealed for tungstenite::protocol::frame::Utf8Bytes {}
}

/// Represents an optionally contiguous source of data received from a client.
///
/// The methods defined in the `Payload` trait also provide counterparts with
/// zeroization guarantees, ensuring that the original buffers are securely
/// cleared after the data is read.
///
/// # Memory Hygiene
///
/// Payload methods take ownership of `self` to prevent accidental reuse of
/// volatile buffers. This behavior ensures that once the data is coalesced or
/// deserialized, the original memory is unreachable.
///
/// ## Zeroization
///
/// The majority of use-cases where zeroization is preferred but not strictly
/// necessary can benefit from using the `be_z_*` prefixed versions of the
/// methods defined in the `Payload` trait. The `be_z` prefix stands for
/// "best-effort zeroization". If zeroization is impossible due to non-unique
/// access of a buffer contained in the payload, `be_z_*` variations fall back
/// to their non-zeroing counterparts.
///
/// If zeroization is a hard requirement, we recommend defining a policy that
/// is sufficient for your business use-case. For example, returning an opaque
/// 500 error to the client and immediately stopping request processing is likely
/// enough to satisfy the definition of "fair handling of user data". As always,
/// we suggest defining a policy and working with compliance and legal to
/// determine what is right for your situation.
///
/// In any case, users should avoid retaining the payload returned in the `Err`
/// branch of strict zeroizing methods prefixed by `z_*` and stop processing
/// the request as soon as possible. This reduces the likelihood of a panic
/// crashing a connection task, potentially (albeit unlikely) exposing
/// un-zeroed memory.
///
pub trait Payload: sealed::Sealed + Sized {
    /// Coalesces all non-contiguous bytes into a single contiguous `Vec<u8>`.
    ///
    fn coalesce(self) -> Vec<u8>;

    /// Coalesces all non-contiguous bytes into a single contiguous `Vec<u8>`.
    ///
    /// If zeroization is impossible due to non-unique access of an underlying
    /// frame buffer, `self` is returned to the caller.
    ///
    /// # Security
    ///
    /// Users should avoid retaining the returned `Self` in `Err` longer than
    /// necessary, as it contains un-zeroed memory.
    ///
    fn z_coalesce(self) -> Result<Vec<u8>, Self>;

    /// Deserialize the payload as JSON into the specified type `T`.
    ///
    /// # Errors
    ///
    /// - `Err(Error)` if `T` cannot be deserialized from the data in `self`
    ///
    fn json<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        deserialize_json(&self.coalesce())
    }

    /// Deserialize the payload as JSON into the specified type `T`, zeroizing
    /// the original data from which the `T` is deserialized.
    ///
    /// # Errors
    ///
    /// - `Err(Self)` if zeroization is impossible due to non-unique access
    /// - `Ok(Err(Error))` if `T` cannot be deserialized from the data in `self`
    ///
    /// # Security
    ///
    /// Users should avoid retaining the returned `Self` in `Err` longer than
    /// necessary, as it contains un-zeroed memory.
    ///
    fn z_json<T>(self) -> Result<Result<T, Error>, Self>
    where
        T: DeserializeOwned,
    {
        self.z_coalesce().map(|data| deserialize_json(&data))
    }

    /// Deserialize the payload as JSON into the specified type `T`, zeroizing
    /// the original data from which the `T` is deserialized.
    ///
    /// If zeroization is impossible due to non-unique access, fallback to
    /// [`Payload::json`].
    ///
    /// # Errors
    ///
    /// - `Err(Error)` if `T` cannot be deserialized from the data in `self`
    ///
    fn be_z_json<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        self.z_json().unwrap_or_else(Self::json)
    }

    /// Converts the payload into a UTF-8 `String`.
    ///
    /// # Errors
    ///
    /// - `Err(Error)` if the payload contains an invalid UTF-8 byte sequence
    ///
    fn utf8(self) -> Result<String, Error> {
        deserialize_utf8(self.coalesce())
    }

    /// Converts the payload into a UTF-8 `String`, zeroizing the original data
    /// from which the `String` is constructed.
    ///
    /// # Errors
    ///
    /// - `Err(Self)` if zeroization is impossible due to non-unique access
    /// - `Ok(Err(Error))` if the payload contains an invalid UTF-8 byte
    ///   sequence
    ///
    fn z_utf8(self) -> Result<Result<String, Error>, Self> {
        self.z_coalesce().map(deserialize_utf8)
    }

    /// Converts the payload into a UTF-8 `String`, zeroizing the original data
    /// from which the `String` is constructed.
    ///
    /// If zeroization is impossible due to non-unique access, fallback to
    /// [`Payload::utf8`].
    ///
    /// # Errors
    ///
    /// - `Err(Error)` if the payload contains an invalid UTF-8 byte sequence
    ///
    fn be_z_utf8(self) -> Result<String, Error> {
        self.z_utf8().unwrap_or_else(Self::utf8)
    }
}

/// The data and trailers of a request body.
///
pub struct Aggregate {
    payload: RequestPayload,
    _unsend: PhantomData<Rc<()>>,
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Coalesce {
    body: Limited<Incoming>,
    payload: Option<RequestPayload>,
}

struct RequestPayload {
    frames: SmallVec<[Bytes; 1]>,
    trailers: Option<HeaderMap>,
}

fn already_read<T>() -> Result<T, Error> {
    raise!(500, message = "The request body has already been read.")
}

fn deserialize_json<T>(buf: &[u8]) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    #[derive(Deserialize)]
    struct Tagged<D> {
        data: D,
    }

    match serde_json::from_slice(buf) {
        Ok(Tagged { data }) => Ok(data),
        Err(error) => raise!(400, error),
    }
}

#[inline]
fn deserialize_utf8(data: Vec<u8>) -> Result<String, Error> {
    String::from_utf8(data).or_else(|error| raise!(400, error.utf8_error()))
}

fn into_future_error<T>(error: BoxError) -> Result<T, Error> {
    if error.is::<LengthLimitError>() {
        // Payload Too Large
        raise!(413, boxed = error);
    }

    // Bad Request
    raise!(400, boxed = error);
}

/// Zeroize the buffer backing the provided `Bytes`. Afterwards, the data in
/// `frame` is unreachable and the visible length is 0.
///
/// Adapted from the [zeroize] crate in order to prevent an O(n) call to
/// compiler_fence where n is the number of frames in a payload.
///
/// To safely call this fn, you must guarantee the following invariants:
///
///   1. `Bytes::is_unique` is true for `frame`
///   2. `compiler_fence` is called after each frame is zeroized
///
/// [zeroize]: https://crates.io/crates/zeroize/
#[inline(never)]
unsafe fn unfenced_zeroize(frame: &mut Bytes) {
    let len = frame.remaining();
    let ptr = frame.as_ptr() as *mut u8;

    for idx in 0..len {
        unsafe {
            ptr::write_volatile(ptr.add(idx), 0);
        }
    }

    // Make the visible length of the frame buffer 0.
    frame.advance(len);
}

#[inline(always)]
fn release_compiler_fence() {
    compiler_fence(Ordering::Release);
}

/// Converts a `Bytes` instance that was previously wrapped in a `ByteString`
/// back to a `ByteString`.
///
/// *Note: This fn is safe as long as `bytes` is valid UTF-8.*
///
#[cfg(feature = "ws")]
fn back_to_utf8_bytes(bytes: Bytes) -> Utf8Bytes {
    // Safety:
    //
    // We know this is safe because `self` is guaranteed to be
    // valid UTF-8 and z_json failed before zeroizing the backing
    // buffer.
    unsafe { Utf8Bytes::from_bytes_unchecked(bytes) }
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
        self.payload
            .iter()
            .map(Buf::remaining)
            .try_fold(0usize, |len, remaining| len.checked_add(remaining))
    }
}

impl Aggregate {
    fn new(payload: RequestPayload) -> Self {
        Self {
            payload,
            _unsend: PhantomData,
        }
    }
}

impl Payload for Aggregate {
    fn coalesce(mut self) -> Vec<u8> {
        let mut dest = self.len().map(Vec::with_capacity).unwrap_or_default();

        for frame in self.payload.iter_mut() {
            // The transport layer sufficiently chunks each frame.
            dest.extend_from_slice(frame.as_ref());

            // Make the visible length of the frame buffer 0.
            frame.advance(frame.remaining());
        }

        dest
    }

    fn z_coalesce(mut self) -> Result<Vec<u8>, Self> {
        let mut dest = self.len().map(Vec::with_capacity).unwrap_or_default();

        // If we do not have unique access to each frame in self, return back
        // to the caller.
        if !self.payload.iter().all(Bytes::is_unique) {
            return Err(self);
        }

        for frame in self.payload.iter_mut() {
            // The transport layer sufficiently chunks each frame.
            dest.extend_from_slice(frame.as_ref());

            // Safety:
            //
            // The precondition at the top of this function ensures that we
            // have unique access to each frame contained in self.
            //
            // Since Aggregate is also !Send + !Sync, it is impossible to wrap
            // an instance of Aggregate in an Arc and send or share a clone of
            // self with another task.
            //
            // The combination of the aforementioned proofs confirms that we
            // can safely mutate the buffer backing each frame in the payload.
            unsafe {
                unfenced_zeroize(frame);
            }
        }

        // Ensures sequential access to the buffers contained in self.
        // A necessary step after zeroization.
        release_compiler_fence();

        Ok(dest)
    }
}

impl Coalesce {
    pub(super) fn new(body: Limited<Incoming>) -> Self {
        Self {
            body,
            payload: Some(RequestPayload::new()),
        }
    }
}

impl Future for Coalesce {
    type Output = Result<Aggregate, Error>;

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        let Self { body, payload } = self.get_mut();
        let mut body = Pin::new(body);

        loop {
            let Some(result) = ready!(body.as_mut().poll_frame(context)) else {
                return Poll::Ready(match payload.take() {
                    Some(payload) => Ok(Aggregate::new(payload)),
                    None => already_read(),
                });
            };

            let frame = result.or_else(into_future_error)?;
            let payload = payload.as_mut().map_or_else(already_read, Ok)?;
            let trailers = match frame.into_data() {
                Ok(data) => {
                    payload.frames.push(data);
                    continue;
                }
                Err(frame) => {
                    let Ok(trailers) = frame.into_trailers() else {
                        return Poll::Ready(Err(Error::new(
                            StatusCode::BAD_REQUEST,
                            "unexpected frame type received while reading the request body",
                        )));
                    };

                    trailers
                }
            };

            if let Some(existing) = payload.trailers.as_mut() {
                existing.extend(trailers);
            } else {
                payload.trailers = Some(trailers);
            }
        }
    }
}

impl RequestPayload {
    fn new() -> Self {
        Self {
            frames: SmallVec::new(),
            trailers: None,
        }
    }

    #[inline]
    fn iter(&self) -> slice::Iter<'_, Bytes> {
        self.frames.iter()
    }

    #[inline]
    fn iter_mut(&mut self) -> slice::IterMut<'_, Bytes> {
        self.frames.iter_mut()
    }
}

#[cfg(feature = "ws")]
impl Payload for Utf8Bytes {
    fn coalesce(self) -> Vec<u8> {
        let mut src = Bytes::from(self);
        let mut dest = Vec::with_capacity(src.remaining());

        // The transport layer sufficiently chunks each frame.
        dest.extend_from_slice(src.as_ref());

        // Make the visible length of the frame buffer 0.
        src.advance(src.remaining());

        dest
    }

    fn z_coalesce(self) -> Result<Vec<u8>, Self> {
        let mut src = Bytes::from(self);

        // If we do not have unique access to self, return back to the caller.
        if !src.is_unique() {
            return Err(back_to_utf8_bytes(src));
        }

        let mut dest = Vec::with_capacity(src.remaining());

        // The transport layer sufficiently chunks each frame.
        dest.extend_from_slice(src.as_ref());

        // Safety:
        //
        // The precondition at the top of this function ensures that we
        // have unique access to self and therefore, can mutate the buffer.
        unsafe {
            unfenced_zeroize(&mut src);
        }

        // Ensures sequential access to the buffers contained in self.
        // A necessary step after zeroization.
        release_compiler_fence();

        Ok(dest)
    }

    fn json<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let mut src = Bytes::from(self);

        // Attempt to deserialize `T` from the bytes in self.
        let result = deserialize_json(src.as_ref());

        // Make the visible length of the frame buffer 0.
        src.advance(src.remaining());

        result
    }

    fn z_json<T>(self) -> Result<Result<T, Error>, Self>
    where
        T: DeserializeOwned,
    {
        let mut src = Bytes::from(self);

        // If we do not have unique access to self, return back to the caller.
        if !src.is_unique() {
            return Err(back_to_utf8_bytes(src));
        }

        // Attempt to deserialize `T` from the bytes in self.
        let result = deserialize_json(src.as_ref());

        // Safety:
        //
        // The precondition at the top of this function ensures that we
        // have unique access to self and therefore, can mutate the buffer.
        unsafe {
            unfenced_zeroize(&mut src);
        }

        // Ensures sequential access to the buffers contained in self.
        // A necessary step after zeroization.
        release_compiler_fence();

        Ok(result)
    }
}
