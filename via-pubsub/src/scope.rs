use std::collections::HashSet;
use std::hash::Hash;
use std::marker::PhantomData;

use crate::pubsub::{Publish, Receive, Result};
use crate::sign::{OurEvent, PeerEvent, RawPeerEvent};

/// The initial capacity of the `HashSet` used to filter a `Scope`.
const INIT_INTEREST_FILTER_CAP: usize = 100;

pub trait Scope<T> {
    fn register(&mut self, interest: T);
    fn deregister(&mut self, interest: &T);
}

pub(crate) struct Filter<Interest, Schema, T> {
    actor: Interest,
    source: T,
    interests: HashSet<Interest>,
    _schema_ty: PhantomData<Schema>,
}

impl<Interest, Schema, T> Filter<Interest, Schema, T>
where
    Interest: Clone + Eq + Hash,
{
    pub(crate) fn new(actor: Interest, source: T) -> Self {
        Self {
            actor,
            source,
            interests: HashSet::with_capacity(INIT_INTEREST_FILTER_CAP),
            _schema_ty: PhantomData,
        }
    }

    fn try_claim(&self, event: RawPeerEvent<Interest>) -> Option<PeerEvent<Interest>> {
        match event {
            RawPeerEvent::Lag(len) => {
                // Lag events are always unique to `self.actor`.
                Some(PeerEvent::Lag(len))
            }
            RawPeerEvent::Logout(user) => {
                // Some if `self.actor` is `user`.
                (self.actor == user).then_some(PeerEvent::Logout)
            }
            RawPeerEvent::Relay(interest, message) => {
                // Some if `self.actor` has subscribed to `interest`.
                self.interests
                    .contains(&interest)
                    .then(|| PeerEvent::Relay(message))
            }
            RawPeerEvent::Register(user, interest) => {
                // Some if `user` is `Some(self.actor) | None`.
                user.is_none_or(|id| self.actor == id)
                    .then_some(PeerEvent::Register(interest))
            }
            RawPeerEvent::Deregister(user, interest) => {
                // Some if `user` is `Some(self.actor) | None`.
                user.is_none_or(|id| self.actor == id)
                    .then_some(PeerEvent::Deregister(interest))
            }
        }
    }
}

impl<Interest, Schema, T> Publish<OurEvent<Interest, Schema>> for Filter<Interest, Schema, T>
where
    T: Publish<OurEvent<Interest, Schema>>,
{
    fn send(&self, event: OurEvent<Interest, Schema>) -> impl Future<Output = Result> + Send {
        self.source.send(event)
    }
}

impl<Interest, Schema, T> Receive for Filter<Interest, Schema, T>
where
    Interest: Clone + Eq + Hash + Send,
    Schema: Send,
    T: Receive<Event = RawPeerEvent<Interest>> + Send,
{
    type Event = PeerEvent<Interest>;

    async fn recv(&mut self) -> Result<Option<Self::Event>> {
        match self.source.recv().await {
            Ok(Some(event)) => Ok(self.try_claim(event)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn try_recv(&mut self) -> Result<Option<Self::Event>> {
        match self.source.try_recv() {
            Ok(Some(event)) => Ok(self.try_claim(event)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl<Interest, Schema, T> Scope<Interest> for Filter<Interest, Schema, T>
where
    Interest: Eq + Hash,
{
    fn register(&mut self, interest: Interest) {
        self.interests.insert(interest);
    }

    fn deregister(&mut self, interest: &Interest) {
        self.interests.remove(interest);
    }
}
