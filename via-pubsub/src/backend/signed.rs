use base64::engine::{Engine, general_purpose::STANDARD as base64};
use hmac::{Hmac, KeyInit, Mac};
use http::StatusCode;
use serde::{Serialize, de::DeserializeOwned};
use sha2::Sha256;
use via::Error;
use zeroize::Zeroizing;

use super::{Event, RawPeerEvent};

const DOMAIN: &[u8] = b"via-pubsub\0";
const LEN: usize = 32;

#[derive(Clone)]
pub struct Key {
    bytes: Zeroizing<[u8; LEN]>,
}

#[derive(Clone)]
pub struct Signer {
    key: Key,
    scope: String,
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

impl Key {
    pub fn new(input: &[u8]) -> Result<Self, Error> {
        let mut bytes = Zeroizing::new([0u8; LEN]);

        if base64
            .decode_slice(input, &mut *bytes)
            .is_ok_and(|len| len == LEN)
        {
            Ok(Key { bytes })
        } else {
            Err(invalid_key_len())
        }
    }
}

impl Signer {
    pub fn deserialize<T, U>(&self, payload: &[u8]) -> Result<RawPeerEvent<T>, Error>
    where
        T: DeserializeOwned,
        U: DeserializeOwned + Serialize,
    {
        self.authenticate(payload)
            .and_then(|slice| match serde_json::from_slice(slice) {
                Ok(event) => Event::<T, U>::into_raw(event),
                Err(error) => Err(Error::from_serde_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error,
                )),
            })
    }

    pub fn serialize<T, U>(&self, event: &Event<T, U>) -> Result<(&str, Vec<u8>), Error>
    where
        T: Serialize,
        U: Serialize,
    {
        let mut payload = vec![0; LEN];

        match serde_json::to_writer(&mut payload, event) {
            Ok(_) => {
                let tag = self.mac(&payload[LEN..])?.finalize();
                payload[..LEN].copy_from_slice(tag.as_bytes());
                Ok((self.scope(), payload))
            }
            Err(error) => Err(Error::from_serde_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                error,
            )),
        }
    }
}

impl Signer {
    pub(super) fn new(key: Key, scope: String) -> Self {
        Self { key, scope }
    }

    pub(super) fn scope(&self) -> &str {
        &self.scope
    }

    fn authenticate<'a>(&self, payload: &'a [u8]) -> via::Result<&'a [u8]> {
        let (tag, payload) = payload.split_at_checked(LEN).ok_or_else(invalid_tag_len)?;
        let Ok(fixed) = tag.try_into() else {
            unreachable!("split_at_checked produces at least one slice with len == mid");
        };

        self.mac(payload)?
            .verify(fixed)
            .map_or_else(|_| Err(unauthorized_tag()), |_| Ok(payload))
    }

    fn mac(&self, payload: &[u8]) -> Result<Hmac<Sha256>, Error> {
        let Ok(mut mac) = Hmac::new_from_slice(&*self.key.bytes) else {
            return Err(invalid_tag_len());
        };

        mac.update(DOMAIN);
        mac.update(&(self.scope().len() as u64).to_be_bytes());
        mac.update(self.scope().as_bytes());
        mac.update(&(payload.len() as u64).to_be_bytes());
        mac.update(payload);

        Ok(mac)
    }
}
