mod shared_str;

pub use shared_str::SharedStr;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ops::ControlFlow;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc};
use via::error::{Catch, Error, Propagate};

use super::id::Id;

pub type Sender = broadcast::Sender<Event>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(content = "data", rename_all = "lowercase", tag = "type")]
pub enum Action {
    Update(SharedStr),
    Subscribe,
    Unsubscribe,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Event {
    interest: Id,
    action: Action,
}

pub struct Pubsub {
    tx: mpsc::Sender<Event>,
    rx: broadcast::Receiver<Event>,
}

pub struct PubsubHandle {
    interests: HashSet<Id>,
    tx: mpsc::Sender<Event>,
    rx: broadcast::Receiver<Event>,
}

impl Event {
    pub fn new(interest: Id, action: Action) -> Self {
        Self { interest, action }
    }

    pub fn action(&self) -> &Action {
        &self.action
    }

    pub fn interest(&self) -> Id {
        self.interest
    }
}

impl Pubsub {
    pub async fn connect(capacity: usize) -> Self {
        let (tx, task_rx) = mpsc::channel(capacity);
        let (task_tx, rx) = broadcast::channel(capacity);

        tokio::spawn({
            let tx = task_tx;
            let mut rx = task_rx;

            Box::pin(async move {
                while let Some(event) = rx.recv().await {
                    println!("event received by task: {:#?}", event);
                }
            })
        });

        Self { tx, rx }
    }

    pub fn subscribe(&self) -> PubsubHandle {
        let interests = HashSet::new();
        let tx = self.tx.clone();
        let rx = self.rx.resubscribe();

        PubsubHandle { interests, tx, rx }
    }
}

impl PubsubHandle {
    pub async fn publish(&self, event: Event) -> Result<(), Catch> {
        self.tx.send(event).await.or_break()
    }

    pub async fn recv(&mut self) -> Result<Option<Event>, Catch> {
        match self.rx.recv().await {
            Ok(event) => {
                if self.interests.contains(&event.interest) {
                    Ok(Some(event))
                } else {
                    Ok(None)
                }
            }
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

    pub fn subscribe(&mut self, interest: Id) {
        self.interests.insert(interest);
    }

    pub fn unsubscribe(&mut self, interest: &Id) {
        self.interests.remove(interest);
    }
}
