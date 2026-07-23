use bytes::Bytes;
use http::StatusCode;
use serde::Serialize;
use std::fmt::{self, Debug, Formatter};
use via::Error;

/// An opaque type that represents a serialized update from a peer.
#[derive(Clone)]
pub struct Opaque(Bytes);

pub(crate) fn serialize(value: &impl Serialize) -> via::Result<Opaque> {
    match serde_json::to_string(value) {
        Ok(string) => Ok(Opaque(string.into())),
        Err(error) => Err(Error::from_serde_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            error,
        )),
    }
}

impl Debug for Opaque {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("Opaque")
    }
}

#[cfg(feature = "tokio-tungstenite")]
impl From<Opaque> for via::ws::Message {
    fn from(value: Opaque) -> Self {
        // Safety: Opaque can only be constructed from valid UTF-8.
        let text = unsafe { via::ws::Utf8Bytes::from_bytes_unchecked(value.0) };

        // Return a text message containing the bytes in self.
        Self::Text(text)
    }
}

#[cfg(feature = "tokio-websockets")]
impl From<Opaque> for via::ws::Message {
    fn from(value: Opaque) -> Self {
        Self::text(value.0)
    }
}
