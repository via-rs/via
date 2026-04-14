use base64::engine::{Engine, general_purpose::STANDARD as base64};

#[cfg(feature = "aws-lc-rs")]
use aws_lc_rs::digest::{Context, SHA1_FOR_LEGACY_USE_ONLY};

#[cfg(feature = "ring")]
use ring::digest::{Context, SHA1_FOR_LEGACY_USE_ONLY};

use crate::ws::error::UpgradeError;

const WS_ACCEPT_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub struct Base64EncodedDigest([u8; 28]);

pub fn sha1(input: &[u8]) -> Result<Base64EncodedDigest, UpgradeError> {
    if !input.is_ascii() {
        return Err(UpgradeError::InvalidAcceptEncoding);
    }

    let mut hasher = Context::new(&SHA1_FOR_LEGACY_USE_ONLY);
    let mut buf = [0; 28];

    hasher.update(input);
    hasher.update(WS_ACCEPT_GUID);

    if base64.encode_slice(hasher.finish(), &mut buf).is_ok() {
        Ok(Base64EncodedDigest(buf))
    } else {
        Err(UpgradeError::EncoderError)
    }
}

impl Base64EncodedDigest {
    #[inline(always)]
    pub fn as_str(&self) -> &str {
        // Safety: Base64 is guaranteed to be ASCII and therefore, valid UTF-8.
        unsafe { str::from_utf8_unchecked(&self.0) }
    }
}
