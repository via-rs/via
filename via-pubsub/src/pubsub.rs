use std::collections::HashSet;
use via::error::Catch;

use crate::backend::{Backend, Event, PeerEvent, RawPeerEvent};

pub struct Pubsub<T> {
    backend: T,
}

pub struct Subscription<T: Backend> {
    actor: T::Interest,
    backend: T,
    interests: HashSet<T::Interest>,
}

impl<T: Backend> Pubsub<T> {
    pub(crate) fn new(backend: T) -> Self {
        Self { backend }
    }

    pub fn dispatch(&self, event: Event<T::Interest, T::Payload>) {
        self.backend.dispatch(event);
    }

    pub fn subscribe(&self, actor: T::Interest) -> Subscription<T> {
        Subscription {
            actor,
            backend: self.backend.subscribe(),
            interests: HashSet::new(),
        }
    }
}

impl<T: Backend> Subscription<T> {
    pub async fn send(&self, event: Event<T::Interest, T::Payload>) -> Result<(), Catch> {
        self.backend.send(event).await
    }

    pub async fn recv(&mut self) -> Result<Option<PeerEvent<T::Interest>>, Catch> {
        self.backend.recv().await.map(|event| match event {
            RawPeerEvent::Logout(ref actor) => (&self.actor == actor).then_some(PeerEvent::Logout),

            RawPeerEvent::Relay(ref interest, payload) => self
                .interests
                .contains(interest)
                .then(|| PeerEvent::Relay(payload)),

            RawPeerEvent::Register(ref actor, ref interest) => actor
                .as_ref()
                .is_none_or(|id| &self.actor == id)
                .then(|| PeerEvent::Register(*interest)),

            RawPeerEvent::Deregister(ref actor, ref interest) => actor
                .as_ref()
                .is_none_or(|id| &self.actor == id)
                .then(|| PeerEvent::Register(*interest)),
        })
    }

    pub fn register(&mut self, interest: T::Interest) {
        self.interests.insert(interest);
    }

    pub fn deregister(&mut self, interest: &T::Interest) {
        self.interests.remove(interest);
    }
}
