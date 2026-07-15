#[cfg(feature = "redis")]
mod redis;

#[cfg(feature = "redis")]
pub use redis::Redis;

use serde::{Deserialize, Serialize};
use std::hash::Hash;

use crate::opaque::Opaque;
use crate::pubsub::{Receiver, Sender};

pub trait Backend<T, U> {
    #[allow(async_fn_in_trait)]
    async fn connect(&self) -> via::Result<(Sender<T, U>, Receiver<T>)>;
}

pub trait Interest: Copy + Eq + Hash + Serialize {}

#[derive(Clone, Debug)]
pub enum Event<T> {
    Logout(T),
    Relay(T, Opaque),
    Register(T, T),
    Deregister(T, T),
}

#[derive(Serialize)]
pub struct Publish<T, U> {
    interest: T,
    action: RawAction<T, U>,
}

#[derive(Deserialize, Serialize)]
#[serde(content = "data", rename_all = "lowercase", tag = "type")]
enum RawAction<T, U> {
    Logout,
    Relay(U),
    Register(T),
    Deregister(T),
}

impl<T, U> Publish<T, U> {
    pub fn logout(actor: T) -> Self {
        Self {
            interest: actor,
            action: RawAction::Logout,
        }
    }

    pub fn relay(interest: T, payload: U) -> Self {
        Self {
            interest,
            action: RawAction::Relay(payload),
        }
    }

    pub fn register(interest: T, actor: T) -> Self {
        Self {
            interest,
            action: RawAction::Register(actor),
        }
    }

    pub fn deregister(interest: T, actor: T) -> Self {
        Self {
            interest,
            action: RawAction::Deregister(actor),
        }
    }
}

impl Interest for i64 {}
