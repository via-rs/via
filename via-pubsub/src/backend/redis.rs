use redis::aio::{ConnectionManager, ConnectionManagerConfig, SendError};
use redis::{AsyncTypedCommands, Client, PushInfo, PushKind, RedisResult, Value};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Error as JsonError;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::ControlFlow;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use via::{Error, error::Catch};

use super::{Backend, Event, PeerEvent};
use crate::pubsub::Pubsub;
use crate::util::error;

pub struct Builder<T, U> {
    capacity: usize,
    version: Option<u32>,
    topic: Option<String>,
    _ty: PhantomData<(T, U)>,
}

pub struct Redis<T, U> {
    sender: mpsc::Sender<Event<T, U>>,
    receiver: broadcast::Receiver<PeerEvent<T>>,
    _schema_ty: PhantomData<U>,
}

struct Dispatcher<T, U> {
    cast: broadcast::Sender<PeerEvent<T>>,
    inbound: mpsc::Receiver<PushInfo>,
    outbound: mpsc::Receiver<Event<T, U>>,
}

fn deserialize_event<T, U>(input: &str) -> Result<PeerEvent<T>, JsonError>
where
    T: DeserializeOwned + Serialize,
    U: DeserializeOwned + Serialize,
{
    serde_json::from_str(input).and_then(Event::<T, U>::into_event)
}

fn require_argument(arg: &str) -> Error {
    Error::new(format!("missing required argument: \"{}\"", arg))
}

fn value_as_str(value: &Value) -> Option<&str> {
    if let Value::BulkString(data) = value {
        str::from_utf8(data.as_ref()).ok()
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

async fn recv<T, U>(mut dispatcher: Dispatcher<T, U>, mut client: ConnectionManager, topic: String)
where
    T: DeserializeOwned + Serialize,
    U: DeserializeOwned + Serialize,
{
    loop {
        tokio::select! {
            incoming = dispatcher.outbound.recv() => {
                let Some(event) = incoming else {
                    break;
                };

                let json = match serde_json::to_string(&event) {
                    Ok(string) => string,
                    Err(error) => {
                        log!(error, "{}", &error);
                        continue;
                    }
                };

                if let Err(error) = client.publish(&topic, &json).await {
                    log!(error, "{}", error);
                }
            }
            outgoing = dispatcher.inbound.recv() => {
                let Some(push) = outgoing else {
                    break;
                };

                if let PushKind::Message = &push.kind
                    && let [t, payload] = &*push.data
                    && value_as_str(t).is_some_and(|s| s == topic)
                    && let Some(input) = value_as_str(payload)
                {
                    match deserialize_event::<T, U>(input) {
                        Ok(event) => {
                            // TODO: implement checksum verification.
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
            _schema_ty: PhantomData,
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
    async fn recv(&mut self) -> Result<PeerEvent<T>, Catch> {
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
    pub fn version(mut self, version: u32) -> Self {
        self.version = Some(version);
        self
    }

    pub fn topic(mut self, topic: &str) -> Self {
        self.topic = Some(topic.to_owned());
        self
    }

    pub async fn connect(self, url: String) -> via::Result<Pubsub<Redis<T, U>>> {
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
                recv(dispatcher, client, topic).await;
            })
        });

        Ok(Pubsub::new(Redis {
            sender,
            receiver,
            _schema_ty: PhantomData,
        }))
    }
}

impl<T, U> Dispatcher<T, U> {
    fn new(
        cast: broadcast::Sender<PeerEvent<T>>,
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
