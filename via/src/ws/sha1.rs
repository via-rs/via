use base64::engine::{Engine, general_purpose::STANDARD as base64};

#[cfg(feature = "aws-lc-rs")]
use aws_lc_rs::digest::{Context, SHA1_FOR_LEGACY_USE_ONLY};

#[cfg(feature = "ring")]
use ring::digest::{Context, SHA1_FOR_LEGACY_USE_ONLY};

use crate::{Error, raise};

const WS_ACCEPT_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn sha1(input: &[u8]) -> Result<[u8; 28], Error> {
    let mut hasher = Context::new(&SHA1_FOR_LEGACY_USE_ONLY);
    let mut buf = [0; 28];

    hasher.update(input);
    hasher.update(WS_ACCEPT_GUID);

    let Ok(_) = base64.encode_slice(hasher.finish(), &mut buf) else {
        raise!(message = "an error occurred while generating the websocket accept key.");
    };

    Ok(buf)
}
