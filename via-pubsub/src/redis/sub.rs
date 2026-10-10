use serde::{Serialize, de::DeserializeOwned};
use std::hash::Hash;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::{broadcast, mpsc};

use crate::error::sender_dropped;
use crate::pubsub::{Publish, Receive, Result};
use crate::scope::{Filter, Scope};
use crate::sign::{OurEvent, PeerEvent, RawPeerEvent};

pub struct Subscription<T, U> {
    pub(super) scope: Filter<T, U, Channel<T, U>>,
}

pub(super) struct Channel<T, U> {
    pub(super) sender: mpsc::Sender<OurEvent<T, U>>,
    pub(super) receiver: broadcast::Receiver<RawPeerEvent<T>>,
}

impl<T, U> Publish<OurEvent<T, U>> for Channel<T, U>
where
    T: Send,
    U: Send,
{
    async fn send(&self, event: OurEvent<T, U>) -> Result {
        self.sender.send(event).await.map_err(|_| sender_dropped())
    }
}

impl<T, U> Receive for Channel<T, U>
where
    T: Clone + Eq + Hash + Send,
    U: Send,
{
    type Event = RawPeerEvent<T>;

    async fn recv(&mut self) -> Result<Option<Self::Event>> {
        match self.receiver.recv().await {
            Err(RecvError::Lagged(length)) => Ok(Some(RawPeerEvent::Lag(length))),
            Ok(event) => Ok(Some(event)),
            _ => Err(sender_dropped()),
        }
    }

    fn try_recv(&mut self) -> Result<Option<Self::Event>> {
        match self.receiver.try_recv() {
            Err(TryRecvError::Lagged(len)) => Ok(Some(RawPeerEvent::Lag(len))),
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Closed) => Err(sender_dropped()),
        }
    }
}

impl<T, U> Publish<OurEvent<T, U>> for Subscription<T, U>
where
    T: Send,
    U: DeserializeOwned + Serialize + Send,
{
    fn send(&self, event: OurEvent<T, U>) -> impl Future<Output = Result> + Send {
        self.scope.send(event)
    }
}

impl<T, U> Receive for Subscription<T, U>
where
    T: Clone + Eq + Hash + Send,
    U: Send,
{
    type Event = PeerEvent<T>;

    fn recv(&mut self) -> impl Future<Output = Result<Option<Self::Event>>> + Send {
        self.scope.recv()
    }

    fn try_recv(&mut self) -> Result<Option<PeerEvent<T>>> {
        self.scope.try_recv()
    }
}

impl<T, U> Scope<T> for Subscription<T, U>
where
    T: Eq + Hash + Send,
{
    fn register(&mut self, interest: T) {
        self.scope.register(interest);
    }

    fn deregister(&mut self, interest: &T) {
        self.scope.deregister(interest);
    }
}
