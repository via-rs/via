use bytes::Bytes;
use bytestring::ByteString;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SharedStr(ByteString);

impl SharedStr {
    fn into_bytes(self) -> Bytes {
        self.0.into_bytes()
    }
}

impl From<String> for SharedStr {
    fn from(value: String) -> Self {
        SharedStr(From::from(value))
    }
}

#[cfg(feature = "tokio-tungstenite")]
impl From<SharedStr> for via::ws::Message {
    fn from(shared: SharedStr) -> Self {
        use via::ws::Utf8Bytes;

        let bytes = shared.into_bytes();

        // Safety:
        //
        // The bytestream::ByteStream type is built on the same invaraints
        // as the Utf8Bytes type.
        //
        // Therefore, we can safely assume that the bytes returned from
        // ByteString::into_bytes is valid UTF-8.
        let text = unsafe { Utf8Bytes::from_bytes_unchecked(bytes) };

        Self::Text(text)
    }
}
