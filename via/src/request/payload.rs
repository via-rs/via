use bytes::{Buf, Bytes};
use http::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use zeroize::Zeroize;

#[cfg(feature = "tokio-tungstenite")]
use tungstenite::protocol::Message;

#[cfg(feature = "tokio-websockets")]
use tokio_websockets::Message;

use super::body::Aggregate;
use crate::error::Error;
use crate::util::sealed;

#[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
sealed!(Aggregate, Bytes, Message);

#[cfg(not(any(feature = "tokio-tungstenite", feature = "tokio-websockets")))]
sealed!(Aggregate, Bytes);

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

#[derive(Deserialize)]
struct JsonData<T> {
    data: T,
}

#[inline]
fn deserialize_json<T>(buf: &[u8]) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    serde_json::from_slice(buf)
        .map_err(|error| Error::from_serde_json(StatusCode::BAD_REQUEST, error))
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

        for frame in self.payload_mut().frames_mut().iter_mut() {
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
        if let Some(frame) = self.payload_mut().unary_mut() {
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

        // If we do not have unique access to each frame in self, return back
        // to the caller.
        if !self.payload().frames().iter().all(Bytes::is_unique) {
            return Err(self);
        }

        for frame in self.payload_mut().frames_mut().iter_mut() {
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
        if let Some(frame) = self.payload_mut().unary_mut() {
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
