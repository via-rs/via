use redis::aio::{ConnectionManager, ConnectionManagerConfig, SendError};
use redis::{AsyncTypedCommands, Client, PushInfo, PushKind, RedisResult, Value};
use serde::{Serialize, de::DeserializeOwned};
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::ControlFlow;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use via::error::{Catch, Error};

use super::{Backend, Event, RawPeerEvent, Signer};
use crate::pubsub::Pubsub;
use crate::util::error;

pub struct Builder<T, U> {
    capacity: usize,
    version: Option<u32>,
    signer: Result<Option<Signer>, Error>,
    topic: Option<String>,
    _ty: PhantomData<(T, U)>,
}

pub struct Redis<T, U> {
    sender: mpsc::Sender<Event<T, U>>,
    receiver: broadcast::Receiver<RawPeerEvent<T>>,
}

struct Dispatcher<T, U> {
    cast: broadcast::Sender<RawPeerEvent<T>>,
    inbound: mpsc::Receiver<PushInfo>,
    outbound: mpsc::Receiver<Event<T, U>>,
}

fn require_argument(arg: &str) -> Error {
    Error::new(format!("missing required argument: \"{}\"", arg))
}

fn push_as_message(info: &PushInfo) -> Option<(&str, &[u8])> {
    if let PushKind::Message = &info.kind
        && let [Value::BulkString(topic), Value::BulkString(payload)] = &*info.data
        && let Ok(topic) = str::from_utf8(topic)
    {
        Some((topic, payload.as_ref()))
    } else {
        None
    }
}

async fn connect(url: &str) -> RedisResult<(ConnectionManager, mpsc::Receiver<PushInfo>)> {
    // The channel used internally by the redis client.
    let (tx, pushes) = mpsc::channel(1);
    let config = ConnectionManagerConfig::new()
        .set_push_sender(move |info| tx.try_send(info).or(Err(SendError)))
        .set_automatic_resubscription()
        .set_connection_timeout(Some(Duration::from_secs(5)))
        .set_response_timeout(Some(Duration::from_secs(5)));

    let client = Client::open(url)?
        .get_connection_manager_with_config(config)
        .await?;

    Ok((client, pushes))
}

async fn recv<T, U>(
    mut dispatcher: Dispatcher<T, U>,
    mut client: ConnectionManager,
    signer: Signer,
    topic: String,
) where
    T: DeserializeOwned + Serialize,
    U: DeserializeOwned + Serialize,
{
    loop {
        tokio::select! {
            incoming = dispatcher.outbound.recv() => {
                let Some(event) = incoming else {
                    break;
                };

                let topic = topic.as_bytes();

                match signer.serialize(topic, &event) {
                    Ok(json) => {
                        if let Err(error) = client.publish(topic, json).await {
                            log!(error, "{}", error);
                        }
                    }
                    Err(error) => {
                        log!(error, "{}", &error);
                    }
                }

            }
            outgoing = dispatcher.inbound.recv() => {
                let Some(push) = outgoing else {
                    break;
                };

                if let Some((t, payload)) = push_as_message(&push) && t == &*topic {
                    match signer.deserialize::<T, U>(topic.as_bytes(), payload) {
                        Ok(event) => {
                            let _ = dispatcher.cast.send(event);
                        }
                        Err(error) => {
                            log!(error, "{}", error);
                        }
                    }
                }
            }
        }
    }
}

impl<T, U> Redis<T, U> {
    pub fn builder(capacity: usize) -> Builder<T, U> {
        Builder {
            capacity,
            version: None,
            signer: Ok(None),
            topic: None,
            _ty: PhantomData,
        }
    }
}

impl<T, U> Backend for Redis<T, U>
where
    T: Copy + Eq + Hash + DeserializeOwned + Serialize + Send + 'static,
    U: DeserializeOwned + Serialize + Send + 'static,
{
    type Interest = T;
    type Payload = U;

    fn subscribe(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            receiver: self.receiver.resubscribe(),
        }
    }

    fn dispatch(&self, event: Event<Self::Interest, Self::Payload>) {
        let tx = self.sender.clone();

        tokio::spawn(async move {
            if tx.send(event).await.is_err() {
                log!(warn, "unable to send from detached task. sender dropped.");
            }
        });
    }

    async fn send(&self, event: Event<T, U>) -> Result<(), Catch> {
        self.sender.send(event).await.map_err(error::sender_dropped)
    }

    #[inline]
    async fn recv(&mut self) -> Result<RawPeerEvent<T>, Catch> {
        self.receiver.recv().await.map_err(|error| {
            if let broadcast::error::RecvError::Lagged(n) = error {
                let message = format!("capacity too small. skipped {} messages.", n);
                ControlFlow::Continue(Error::new(message))
            } else {
                error::sender_dropped(0)
            }
        })
    }
}

impl<T, U> Builder<T, U>
where
    T: Copy + Eq + Hash + DeserializeOwned + Serialize + Send + Sync + 'static,
    U: DeserializeOwned + Serialize + Send + Sync + 'static,
{
    pub async fn connect(self, url: String) -> via::Result<Pubsub<Redis<T, U>>> {
        // Confirm that the a valid signer was provided.
        let signer = self.signer?.ok_or_else(|| require_argument("signer"))?;

        // Build namespaced topic name with the topic and version args.
        let topic = format!(
            "via-pub-sub:{}:{}",
            self.topic.ok_or_else(|| require_argument("topic"))?,
            self.version.ok_or_else(|| require_argument("version"))?,
        );

        // Used by subscribers to receive updates from the redis task.
        let (cast, receiver) = broadcast::channel(1);

        // Used by subscribers to send updates to the redis task.
        let (sender, outbound) = mpsc::channel(self.capacity);

        tokio::spawn({
            // Create the redis client and establish a connection.
            let (mut client, inbound) = connect(&url).await?;

            // Create a dispatcher with the channel deps of the redis task.
            let dispatcher = Dispatcher::new(cast, inbound, outbound);

            // Subscribe to the update topic.
            client.subscribe(&topic).await?;

            // Pin the task on the heap. It should live as long as the process.
            Box::pin(async {
                // Start receiving updates from peers.
                recv(dispatcher, client, signer, topic).await;
            })
        });

        Ok(Pubsub::new(Redis { sender, receiver }))
    }

    pub fn version(mut self, version: u32) -> Self {
        self.version = Some(version);
        self
    }

    pub fn signer(mut self, bytes: &[u8]) -> Self {
        self.signer = Signer::new(bytes).map(Some);
        self
    }

    pub fn topic(mut self, topic: &str) -> Self {
        self.topic = Some(topic.to_owned());
        self
    }
}

impl<T, U> Dispatcher<T, U> {
    fn new(
        cast: broadcast::Sender<RawPeerEvent<T>>,
        inbound: mpsc::Receiver<PushInfo>,
        outbound: mpsc::Receiver<Event<T, U>>,
    ) -> Self {
        Self {
            cast,
            inbound,
            outbound,
        }
    }
}
