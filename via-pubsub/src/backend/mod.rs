mod signed;

#[cfg(feature = "redis")]
mod redis;

#[cfg(feature = "redis")]
pub use redis::Redis;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use via::error::Catch;

use crate::util::opaque::{self, Opaque};

#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct Event<T, U>(RawEvent<T, U>);

pub trait Backend {
    type Interest: Copy + Eq + Hash + DeserializeOwned + Serialize;
    type Payload: DeserializeOwned + Serialize;

    fn subscribe(&self) -> Self;

    fn dispatch(&self, event: Event<Self::Interest, Self::Payload>);

    #[allow(async_fn_in_trait)]
    async fn send(&self, event: Event<Self::Interest, Self::Payload>) -> Result<(), Catch>;

    #[allow(async_fn_in_trait)]
    async fn recv(&mut self) -> Result<RawPeerEvent<Self::Interest>, Catch>;
}

#[derive(Clone, Debug)]
pub enum PeerEvent<T> {
    Logout,
    Relay(Opaque),
    Register(T),
    Deregister(T),
}

#[derive(Clone)]
pub enum RawPeerEvent<T> {
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

impl<T, U> Event<T, U> {
    pub fn logout(actor: T) -> Self {
        Self(RawEvent::Logout(actor))
    }

    pub fn relay(interest: T, payload: U) -> Self {
        Self(RawEvent::Relay(interest, payload))
    }

    pub fn register(actor: Option<T>, interest: T) -> Self {
        Self(RawEvent::Register(actor, interest))
    }

    pub fn deregister(actor: Option<T>, interest: T) -> Self {
        Self(RawEvent::Deregister(actor, interest))
    }
}

impl<T, U: Serialize> Event<T, U> {
    fn into_raw(self) -> via::Result<RawPeerEvent<T>> {
        match self.0 {
            RawEvent::Logout(actor) => Ok(RawPeerEvent::Logout(actor)),
            RawEvent::Relay(interest, ref data) => {
                opaque::serialize(data).map(|payload| RawPeerEvent::Relay(interest, payload))
            }
            RawEvent::Register(actor, interest) => Ok(RawPeerEvent::Register(actor, interest)),
            RawEvent::Deregister(actor, interest) => Ok(RawPeerEvent::Deregister(actor, interest)),
        }
    }
}
