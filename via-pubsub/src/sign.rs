use base64::engine::{Engine, general_purpose::STANDARD as base64};
use bytes::Bytes;
use hmac::{Hmac, KeyInit, Mac};
use http::StatusCode;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use std::fmt::{self, Debug, Display, Formatter};
use via::Error;
use zeroize::Zeroizing;

const DOMAIN: &[u8] = b"via-pubsub\0";
const LEN: usize = 32;

#[derive(Debug)]
pub(crate) struct InvalidKeyError;

#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct OurEvent<T, U>(RawEvent<T, U>);

/// An opaque type that represents a serialized update from a peer.
#[derive(Clone)]
pub struct Opaque(Bytes);

#[derive(Clone, Debug)]
pub enum PeerEvent<T> {
    Lag(u64),
    Logout,
    Relay(Opaque),
    Register(T),
    Deregister(T),
}

pub struct Key {
    bytes: Zeroizing<[u8; LEN]>,
}

pub struct Signer {
    key: Key,
    scope: String,
}

#[derive(Clone)]
pub(crate) enum RawPeerEvent<T> {
    Lag(u64),
    Logout(T),
    Relay(T, Opaque),
    Register(Option<T>, T),
    Deregister(Option<T>, T),
}

#[derive(Deserialize, Serialize)]
#[serde(content = "data", rename_all = "lowercase", tag = "type")]
enum RawEvent<T, U> {
    Logout(T),
    Relay(T, U),
    Register(Option<T>, T),
    Deregister(Option<T>, T),
}

fn invalid_tag_len() -> Error {
    Error::new(format!("tag len must be {}", LEN))
}

fn ser_error(error: serde_json::Error) -> via::Error {
    Error::from_serde_json(StatusCode::INTERNAL_SERVER_ERROR, error)
}

fn unauthorized_tag() -> Error {
    Error::new("unauthorized tag".to_owned())
}

impl<T, U> OurEvent<T, U> {
    pub fn logout(actor: T) -> Self {
        Self(RawEvent::Logout(actor))
    }

    pub fn relay(interest: T, schema: U) -> Self {
        Self(RawEvent::Relay(interest, schema))
    }

    pub fn register(actor: Option<T>, interest: T) -> Self {
        Self(RawEvent::Register(actor, interest))
    }

    pub fn deregister(actor: Option<T>, interest: T) -> Self {
        Self(RawEvent::Deregister(actor, interest))
    }
}

impl<T, U: Serialize> OurEvent<T, U> {
    pub(crate) fn into_raw(self) -> via::Result<RawPeerEvent<T>> {
        match self.0 {
            RawEvent::Logout(actor) => Ok(RawPeerEvent::Logout(actor)),
            RawEvent::Relay(interest, ref data) => {
                let json = serde_json::to_string(data).map_err(ser_error)?;
                Ok(RawPeerEvent::Relay(interest, Opaque(json.into())))
            }
            RawEvent::Register(actor, interest) => Ok(RawPeerEvent::Register(actor, interest)),
            RawEvent::Deregister(actor, interest) => Ok(RawPeerEvent::Deregister(actor, interest)),
        }
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

impl Key {
    pub(crate) fn new(input: impl AsRef<[u8]>) -> Result<Self, InvalidKeyError> {
        let mut bytes = Zeroizing::new([0u8; LEN]);

        match base64.decode_slice(input.as_ref(), &mut *bytes) {
            Ok(len) if len == LEN => Ok(Self { bytes }),
            _ => Err(InvalidKeyError),
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
                Ok(event) => OurEvent::<T, U>::into_raw(event),
                Err(error) => Err(Error::from_serde_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error,
                )),
            })
    }

    pub fn serialize<T, U>(&self, event: &OurEvent<T, U>) -> Result<(&str, Vec<u8>), Error>
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
    pub(crate) fn new(key: Key, scope: String) -> Self {
        Self { key, scope }
    }

    pub(crate) fn scope(&self) -> &str {
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

impl std::error::Error for InvalidKeyError {}

impl Display for InvalidKeyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "invalid signing key")
    }
}
