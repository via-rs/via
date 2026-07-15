use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashSet;
use std::hash::Hash;
use std::ops::ControlFlow;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc};
use via::error::{Catch, Error};

use crate::backend::{Backend, Event, Interest, Publish};

pub type Sender<T, U> = mpsc::Sender<Publish<T, U>>;
pub type Receiver<T> = broadcast::Receiver<Event<T>>;

pub struct Pubsub<T, U> {
    sender: Sender<T, U>,
    receiver: Receiver<T>,
}

pub struct Subscription<T, U> {
    actor: T,
    pubsub: Pubsub<T, U>,
    interests: HashSet<T>,
}

fn sender_droped<T>(_: T) -> Catch {
    let message = "pubsub closed".to_owned();
    ControlFlow::Break(Error::new(message))
}

impl<T, U> Pubsub<T, U>
where
    T: Interest,
    U: DeserializeOwned + Serialize,
{
    pub async fn new(backend: impl Backend<T, U>) -> via::Result<Self> {
        let (sender, receiver) = backend.connect().await?;
        Ok(Self { sender, receiver })
    }

    pub async fn publish(&self, event: Publish<T, U>) -> Result<(), Catch> {
        self.sender.send(event).await.map_err(sender_droped)
    }

    pub fn subscribe(&self, actor: T) -> Subscription<T, U> {
        let interests = HashSet::new();
        let pubsub = Self {
            sender: self.sender.clone(),
            receiver: self.receiver.resubscribe(),
        };

        Subscription {
            actor,
            pubsub,
            interests,
        }
    }
}

impl<T: Interest, U> Subscription<T, U> {
    pub fn register(&mut self, interest: T) {
        self.interests.insert(interest);
    }

    pub fn deregister(&mut self, interest: &T) {
        self.interests.remove(interest);
    }

    pub async fn recv(&mut self) -> Result<Option<Event<T>>, Catch> {
        match self.receiver().recv().await {
            Ok(event) => Ok(self.interested_in(&event).then_some(event)),
            Err(error) => {
                if let RecvError::Lagged(n) = error {
                    let message = format!("capacity too small. skipped {} messages.", n);
                    Err(ControlFlow::Continue(Error::new(message)))
                } else {
                    let message = "pubsub closed".to_owned();
                    Err(ControlFlow::Break(Error::new(message)))
                }
            }
        }
    }

    pub async fn send(&self, event: Publish<T, U>) -> Result<(), Catch> {
        self.sender().send(event).await.map_err(sender_droped)
    }
}

impl<T: Interest, U> Subscription<T, U> {
    fn receiver(&mut self) -> &mut Receiver<T> {
        &mut self.pubsub.receiver
    }

    fn sender(&self) -> &Sender<T, U> {
        &self.pubsub.sender
    }

    fn interested_in(&self, event: &Event<T>) -> bool {
        let actor = &self.actor;

        match event {
            Event::Logout(interest) => actor == interest,
            Event::Relay(interest, _) => self.interests.contains(interest),
            Event::Register(primary, secondary) | Event::Deregister(primary, secondary) => {
                self.interests.contains(primary) && actor == secondary
            }
        }
    }
}
