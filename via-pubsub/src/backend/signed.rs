use base64::engine::{Engine, general_purpose::STANDARD as base64};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Serialize, de::DeserializeOwned};
use sha2::Sha256;
use via::Error;
use zeroize::Zeroizing;

use super::{Event, RawPeerEvent};

const DOMAIN: &[u8] = b"via-pubsub\0";
const LEN: usize = 32;

#[derive(Clone)]
pub struct Signer {
    key: Key,
}

#[derive(Clone)]
struct Key {
    bytes: Zeroizing<[u8; LEN]>,
}

#[inline]
fn authenticate<'a>(signer: &Signer, topic: &[u8], payload: &'a [u8]) -> via::Result<&'a [u8]> {
    let (tag, payload) = payload.split_at_checked(LEN).ok_or_else(invalid_tag_len)?;
    let Ok(fixed) = tag.try_into() else {
        unreachable!("split_at and split_at_checked produce at least one slice with len == mid");
    };

    if signer.mac(topic, payload)?.verify(fixed).is_ok() {
        Ok(payload)
    } else {
        Err(unauthorized_tag())
    }
}

fn invalid_key_len() -> Error {
    Error::new(format!("key len must be {}", LEN))
}

fn invalid_tag_len() -> Error {
    Error::new(format!("tag len must be {}", LEN))
}

fn unauthorized_tag() -> Error {
    Error::new("unauthorized tag".to_owned())
}

impl Signer {
    pub(crate) fn new(key: &[u8]) -> via::Result<Self> {
        let mut bytes = Zeroizing::new([0u8; LEN]);

        if base64
            .decode_slice(key, &mut *bytes)
            .is_ok_and(|len| len == LEN)
        {
            Ok(Self { key: Key { bytes } })
        } else {
            Err(invalid_key_len())
        }
    }

    pub fn deserialize<T, U>(&self, topic: &[u8], payload: &[u8]) -> Result<RawPeerEvent<T>, Error>
    where
        T: DeserializeOwned,
        U: DeserializeOwned + Serialize,
    {
        authenticate(self, topic, payload)
            .and_then(|slice| serde_json::from_slice(slice).map_err(Error::from_json))
            .and_then(Event::<T, U>::into_raw)
    }

    pub fn serialize<T, U>(&self, topic: &[u8], event: &Event<T, U>) -> Result<Vec<u8>, Error>
    where
        T: Serialize,
        U: Serialize,
    {
        let mut payload = Vec::new();

        payload.resize(LEN, 0);

        serde_json::to_writer(&mut payload, event).map_err(Error::from_json)?;

        let tag = self.mac(topic, &payload[LEN..])?.finalize();
        payload[..LEN].copy_from_slice(tag.as_bytes());

        Ok(payload)
    }

    fn mac(&self, topic: &[u8], payload: &[u8]) -> Result<Hmac<Sha256>, Error> {
        let Ok(mut mac) = Hmac::new_from_slice(&*self.key.bytes) else {
            return Err(invalid_tag_len());
        };

        mac.update(DOMAIN);
        mac.update(&(topic.len() as u64).to_be_bytes());
        mac.update(topic);
        mac.update(&(payload.len() as u64).to_be_bytes());
        mac.update(payload);

        Ok(mac)
    }
}
