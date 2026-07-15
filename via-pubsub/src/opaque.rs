use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{self, Debug, Formatter};

/// An opaque type that represents a serialized update from a peer.
#[derive(Clone)]
pub struct Opaque(Bytes);

impl Opaque {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Debug for Opaque {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("Opaque")
    }
}

impl<'de> Deserialize<'de> for Opaque {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|utf8| Self(utf8.into()))
    }
}

impl Serialize for Opaque {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Safety: Opaque can only be constructed from valid UTF-8.
        let utf8 = unsafe { str::from_utf8_unchecked(&self.0) };

        // Serialize self as a str.
        serializer.serialize_str(utf8)
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
