use bytes::{Buf, Bytes};
use http::HeaderMap;
use http_body::{Body, Frame, SizeHint};
use hyper::body::Incoming;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, ready};
use std::time::Duration;
use zeroize::Zeroize;

#[cfg(feature = "tokio-tungstenite")]
use tungstenite::protocol::Message;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::Message;

use crate::util::sealed;
use crate::{Error, err};

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
pub trait Payload: sealed::Sealed + Sized {
    /// Coalesces all non-contiguous bytes into a single contiguous `Vec<u8>`.
    ///
    fn coalesce(self) -> Vec<u8>;

    /// Extracts type `T` from a top-level "data" field of a JSON object
    /// contained in `self` and returns it.
    ///
    /// # Errors
    ///
    /// - `Err(Error)` if `T` cannot be deserialized from the "data" in `self`
    ///
    fn data<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        self.json().map(|json: JsonData<T>| json.data)
    }

    /// Deserialize the payload as JSON into the specified type `T`.
    ///
    /// # Errors
    ///
    /// - `Err(Error)` if `T` cannot be deserialized from the data in `self`
    ///
    fn json<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned;

    /// Converts the payload into a UTF-8 `String`.
    ///
    /// # Errors
    ///
    /// - `Err(Error)` if the payload contains an invalid UTF-8 byte sequence
    ///
    fn utf8(self) -> Result<String, Error> {
        deserialize_utf8(self.coalesce())
    }
}

/// Zeroizing variations of the functions provided in `Payload`.
///
/// Unique access to each frame of the payload is required for safe
/// zeroization. If zeroization is a hard requirement, we recommend defining a
/// policy that is sufficient for your business use-case. For example, yielding
/// to runtime and retrying reads when unique access is guaranteed is a viable
/// option for many use-cases. If retaining an unzeroed secret in memory is too
/// risky for your use-case, you can chose to continue processing the request
/// and add a `Connection: close` header to the response or panic to ensure
/// that the memory gets reclaimed by the OS as soon as possible.
///
/// Most of our users just want to do the right thing and zeroize "secrets"
/// such as a password in request payloads when possible. In these cases, it's
/// probably best to avoid decision fatigue and use a "best effort" variation
/// of the function (prefixed by `be_z_*`). They fall back to their non-zeroing
/// counterparts if unique access is not guarateed.
pub trait Payloadz: Payload {
    /// Coalesces all non-contiguous bytes into a single contiguous `Vec<u8>`.
    ///
    /// If zeroization is impossible due to non-unique access of an underlying
    /// frame buffer, `self` is returned to the caller. This allows users to
    /// yield to the runtime and retry zeriozation, add `connection: close` to
    /// the response header, or panic.
    fn z_coalesce(self) -> Result<Vec<u8>, Self>;

    /// Deserialize the payload as JSON into the specified type `T`, zeroizing
    /// the original data from which the `T` is deserialized.
    ///
    /// # Errors
    ///
    /// - `Err(Self)` if zeroization is impossible due to non-unique access
    /// - `Ok(Err(Error))` if `T` cannot be deserialized from the data in `self`
    ///
    /// ## Unique Access
    ///
    /// If zeroization is impossible due to non-unique access of an underlying
    /// frame buffer, `self` is returned to the caller. This allows users to
    /// yield to the runtime and retry zeriozation, add `Connection: close` to
    /// the response header, or panic.
    fn z_data<T>(self) -> Result<Result<T, Error>, Self>
    where
        T: DeserializeOwned,
    {
        self.z_json()
            .map(|result| result.map(|json: JsonData<T>| json.data))
    }

    /// Deserialize the payload as JSON into the specified type `T`, zeroizing
    /// the original data from which the `T` is deserialized.
    ///
    /// # Errors
    ///
    /// - `Err(Self)` if zeroization is impossible due to non-unique access
    /// - `Ok(Err(Error))` if `T` cannot be deserialized from the data in `self`
    ///
    /// ## Unique Access
    ///
    /// If zeroization is impossible due to non-unique access of an underlying
    /// frame buffer, `self` is returned to the caller. This allows users to
    /// yield to the runtime and retry zeriozation, add `Connection: close` to
    /// the response header, or panic.
    fn z_json<T>(self) -> Result<Result<T, Error>, Self>
    where
        T: DeserializeOwned,
    {
        self.z_coalesce()
            .map(|data| deserialize_json(data.as_slice()))
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
    /// ## Unique Access
    ///
    /// If zeroization is impossible due to non-unique access of an underlying
    /// frame buffer, `self` is returned to the caller. This allows users to
    /// yield to the runtime and retry zeriozation, add `Connection: close` to
    /// the response header, or panic.
    fn z_utf8(self) -> Result<Result<String, Error>, Self> {
        self.z_coalesce().map(deserialize_utf8)
    }

    /// Deserialize the payload as JSON into the specified type `T`, zeroizing
    /// the original data from which the `T` is deserialized.
    ///
    /// # Errors
    ///
    /// - `Err(Self)` if zeroization is impossible due to non-unique access
    /// - `Ok(Err(Error))` if `T` cannot be deserialized from the data in `self`
    ///
    /// ## Unique Access
    ///
    /// If zeroization is impossible due to non-unique access of an underlying
    /// frame buffer, `self` is returned to the caller. This allows users to
    /// yield to the runtime and retry zeriozation, add `Connection: close` to
    /// the response header, or panic.
    fn be_z_data<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        self.z_data().unwrap_or_else(Self::data)
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
    body: RequestBody,
}

#[derive(Debug)]
pub struct RequestBody {
    remaining: usize,
    body: Incoming,
    frames: Option<Vec<Bytes>>,
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct WithTrailers {
    body: RequestBody,
    trailers: Option<HeaderMap>,
}

struct RequestPayload {
    frames: Vec<Bytes>,
    trailers: Option<HeaderMap>,
}

#[derive(Deserialize)]
struct JsonData<T> {
    data: T,
}

#[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
sealed!(Aggregate, Bytes, Message);

#[cfg(not(any(feature = "tokio-tungstenite", feature = "tokio-websockets")))]
sealed!(Aggregate, Bytes);

#[inline]
fn deserialize_json<T>(buf: &[u8]) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    serde_json::from_slice(buf).map_err(Error::de_json)
}

#[inline]
fn deserialize_utf8(data: Vec<u8>) -> Result<String, Error> {
    String::from_utf8(data).map_err(|_| Error::invalid_utf8_sequence("request body"))
}

/// Zeroize the buffer backing the provided `Bytes`.
///
/// To safely call this fn, you must guarantee unique access to the buffer that
/// `Bytes` points to. This can be achieved by calling `Bytes::is_unique`.
unsafe fn zeroize_bytes(frame: &mut Bytes) {
    let len = frame.remaining();
    let ptr = frame.as_ptr() as *mut u8;
    let buf = std::ptr::slice_from_raw_parts_mut(ptr, len);

    // Safety:
    // - The allocation backing `frame` is not null
    // - We have unique access to the allocation backing `frame`
    // - The length of `buf` does not exceed the length of `frame`
    Zeroize::zeroize(unsafe { &mut *buf });
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
            .frames()
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

#[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
macro_rules! impl_payload_for_bytes_like {
    ($ty:ty) => {
        impl_payload_for_bytes_like!($ty, |this| this, From::from);
    };
    ($ty:ty, $from:expr) => {
        impl_payload_for_bytes_like!($ty, $from, From::from);
    };
    ($ty:ty, $from:expr, $into:expr) => {
        impl Payload for $ty {
            fn coalesce(self) -> Vec<u8> {
                Payload::coalesce(Bytes::from($from(self)))
            }

            fn data<T>(self) -> Result<T, Error>
            where
                T: DeserializeOwned,
            {
                self.json().map(|json: JsonData<T>| json.data)
            }

            fn json<T>(self) -> Result<T, Error>
            where
                T: DeserializeOwned,
            {
                Payload::json(Bytes::from($from(self)))
            }
        }
    };
}

impl Payload for Aggregate {
    fn coalesce(mut self) -> Vec<u8> {
        let mut dest = self.len().map(Vec::with_capacity).unwrap_or_default();

        for frame in self.payload.frames_mut().iter_mut() {
            // The transport layer sufficiently chunks each frame.
            dest.extend_from_slice(frame.as_ref());

            // Make the visible length of the frame buffer 0.
            frame.advance(frame.remaining());
        }

        dest
    }

    fn json<T>(mut self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        if let [frame] = self.payload.frames_mut() {
            let result = deserialize_json(frame.as_ref());

            // Make the visible length of the frame buffer 0.
            frame.advance(frame.remaining());

            return result;
        }

        deserialize_json(self.coalesce().as_slice())
    }
}

impl Payloadz for Aggregate {
    fn z_coalesce(mut self) -> Result<Vec<u8>, Self> {
        let mut dest = self.len().map(Vec::with_capacity).unwrap_or_default();
        let payload = &mut self.payload;

        // If we do not have unique access to each frame in self, return back
        // to the caller.
        if !payload.frames().iter().all(Bytes::is_unique) {
            return Err(self);
        }

        for frame in payload.frames_mut().iter_mut() {
            // The transport layer sufficiently chunks each frame.
            dest.extend_from_slice(frame.as_ref());

            // Safety:
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
                zeroize_bytes(frame);
            }

            // Make the visible length of the frame buffer 0.
            frame.advance(frame.remaining());
        }

        Ok(dest)
    }

    fn z_json<T>(mut self) -> Result<Result<T, Error>, Self>
    where
        T: DeserializeOwned,
    {
        if let [frame] = self.payload.frames_mut() {
            // If we do not have unique access to the frame, return self back
            // to the caller.
            if !frame.is_unique() {
                return Err(self);
            }

            // Attempt to deserialize `T` from the bytes in self.
            let result = deserialize_json(frame.as_ref());

            // Safety:
            // The precondition at the top of this function ensures that we
            // have unique access to self and therefore, can mutate the buffer.
            unsafe {
                zeroize_bytes(frame);
            }

            // Make the visible length of the frame buffer 0.
            frame.advance(frame.remaining());

            Ok(result)
        } else {
            self.z_coalesce()
                .map(|data| deserialize_json(data.as_slice()))
        }
    }
}

#[cfg(feature = "tokio-tungstenite")]
impl_payload_for_bytes_like!(Message);

#[cfg(feature = "tokio-websockets")]
impl_payload_for_bytes_like!(Message, Message::into_payload, Message::binary);

impl Payload for Bytes {
    fn coalesce(mut self) -> Vec<u8> {
        let mut dest = Vec::with_capacity(self.remaining());

        // The transport layer sufficiently chunks each frame.
        dest.extend_from_slice(self.as_ref());

        // Make the visible length of the frame buffer 0.
        self.advance(self.remaining());

        dest
    }

    fn json<T>(mut self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        // Attempt to deserialize `T` from the bytes in self.
        let result = deserialize_json(self.as_ref());

        // Make the visible length of the frame buffer 0.
        self.advance(self.remaining());

        result
    }
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
            ) -> impl Future<Output = Result<Aggregate, Error>> {
                self.timeout_after(Duration::from_secs(seconds))
            }
        }
    };
}

fn already_read() -> Error {
    err!(500, "a request body can only be read once.")
}

fn unknown_frame_type() -> Error {
    err!(400, "unknown frame type encountered in request.")
}

impl Coalesce {
    pub fn with_trailers(self) -> WithTrailers {
        WithTrailers {
            body: self.body,
            trailers: None,
        }
    }
}

impl_timeout_after!(Coalesce);

impl Coalesce {
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

impl RequestBody {
    /// Aggregate the frames of the request body into a contiguous block of
    /// memory.
    #[inline]
    pub fn coalesce(self) -> Coalesce {
        Coalesce { body: self }
    }
}

impl RequestBody {
    pub(crate) fn new(remaining: usize, body: Incoming, frames: Vec<Bytes>) -> Self {
        Self {
            remaining,
            body,
            frames: Some(frames),
        }
    }

    fn finish(&mut self, trailers: Option<HeaderMap>) -> Result<Aggregate, Error> {
        let frames = self.frames.take().ok_or_else(already_read)?;
        Ok(Aggregate::new(RequestPayload { frames, trailers }))
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

impl RequestPayload {
    #[inline]
    fn frames(&self) -> &[Bytes] {
        &self.frames
    }

    #[inline]
    fn frames_mut(&mut self) -> &mut [Bytes] {
        &mut self.frames
    }
}
