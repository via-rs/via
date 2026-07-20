use redis::aio::{ConnectionManager, ConnectionManagerConfig, SendError};
use redis::{AsyncTypedCommands, Client, PushInfo, PushKind, RedisResult, Value};
use serde::{Serialize, de::DeserializeOwned};
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::ControlFlow;
use tokio::sync::{broadcast, mpsc};
use via::error::{Catch, Error};

use super::signed::{Key, Signer};
use super::{Backend, Event, RawPeerEvent};
use crate::pubsub::Pubsub;
use crate::util::error;

pub struct Builder<T, U> {
    capacity: usize,
    signing_key: Result<Option<Key>, Error>,
    version: Option<u32>,
    _ty: PhantomData<(T, U)>,
}

pub struct Redis<T, U> {
    sender: mpsc::Sender<Event<T, U>>,
    receiver: broadcast::Receiver<RawPeerEvent<T>>,
}

struct Dispatcher<T, U> {
    fanout: broadcast::Sender<RawPeerEvent<T>>,
    inbound: mpsc::Receiver<PushInfo>,
    outbound: mpsc::Receiver<Event<T, U>>,
}

async fn connect(url: &str) -> RedisResult<(ConnectionManager, mpsc::Receiver<PushInfo>)> {
    // The channel used internally by the redis client.
    //
    // A send error means that the recveiver was dropped or the subscriber
    // cannot preserve stream continuity.
    let (tx, pushes) = mpsc::channel(1);

    let config = ConnectionManagerConfig::new()
        .set_pipeline_buffer_size(1)
        .set_concurrency_limit(1)
        .set_push_sender(move |info| tx.try_send(info).or(Err(SendError)))
        //                                                ^^^^^^^^^^^^^^
        // Invalidate the connection; Automatic reconnection and resubscription
        // establish a new stream.
        .set_automatic_resubscription();

    // Create a new redis client and establish a connection with `config`.
    let client = Client::open(url)?
        .get_connection_manager_with_config(config)
        .await?;

    Ok((client, pushes))
}

async fn dispatch<T, U>(mut redis: ConnectionManager, mut trx: Dispatcher<T, U>, signer: Signer)
where
    T: DeserializeOwned + Serialize,
    U: DeserializeOwned + Serialize,
{
    loop {
        tokio::select! {
            // subscriber <- dispatch <- redis
            inbound = trx.inbound.recv() => {
                match inbound
                    .as_ref()
                    .and_then(|push| push_as_message(signer.scope(), push))
                    .map(|payload| signer.deserialize::<_, U>(payload))
                {
                    // Event deserialized successfully.
                    Some(Ok(raw_peer_event)) => {
                        if trx.fanout.send(raw_peer_event).is_err() {
                            break; // Receivers dropped. Don't become a zombie.
                        }
                    }
                    // Deserialization failed.
                    Some(Err(ref error)) => {
                        log!(error, "{}", error);
                    }
                    // Event filtered by scope and type.
                    None => {
                        log!(warn, "event filtered by scope and type.");
                    }
                }
            }

            // subscriber -> dispatch -> redis
            outbound = trx.outbound.recv() => {
                match outbound.as_ref().map(|event| signer.serialize(event)) {
                    // Event serialized successfully.
                    Some(Ok((scope, payload))) => {
                        // Publish to peers. If an error occurs, log it.
                        if let Err(ref error) = redis.publish(scope, payload).await {
                            log!(error, "{}", error);
                        }
                    }
                    // Serialization failed.
                    Some(Err(ref error)) => {
                        log!(error, "{}", error);
                    }
                    // Senders dropped. Don't become a zombie.
                    None => {
                        log!(warn, "senders dropped. stopping redis task.");
                        break;
                    }
                }
            }
        }
    }
}

#[inline]
fn push_as_message<'a>(scope: &str, info: &'a PushInfo) -> Option<&'a [u8]> {
    if let PushKind::Message = &info.kind
        && let [Value::BulkString(topic), Value::BulkString(payload)] = info.data.as_slice()
        && str::from_utf8(topic).is_ok_and(|topic| topic == scope)
    {
        Some(payload.as_ref())
    } else {
        None
    }
}

fn require_argument(arg: &str) -> Error {
    Error::new(format!("missing required argument: \"{}\"", arg))
}

impl<T, U> Redis<T, U> {
    pub fn builder(capacity: usize) -> Builder<T, U> {
        Builder {
            capacity,
            version: None,
            signing_key: Ok(None),
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
        if self.sender.try_send(event).is_err() {
            log!(warn, "failed to synchronously send event.");
        }
    }

    async fn send(&self, event: Event<T, U>) -> Result<(), Catch> {
        self.sender.send(event).await.map_err(error::sender_dropped)
    }

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

    #[inline]
    fn try_recv(&mut self) -> Result<Option<RawPeerEvent<T>>, Catch> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(broadcast::error::TryRecvError::Empty) => Ok(None),
            Err(broadcast::error::TryRecvError::Closed) => Err(error::sender_dropped(0)),
            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                let message = format!("capacity too small. skipped {} messages.", n);
                Err(ControlFlow::Continue(Error::new(message)))
            }
        }
    }
}

impl<T, U> Builder<T, U>
where
    T: Copy + Eq + Hash + DeserializeOwned + Serialize + Send + Sync + 'static,
    U: DeserializeOwned + Serialize + Send + Sync + 'static,
{
    pub async fn connect(self, url: &str, scope: &str) -> via::Result<Pubsub<Redis<T, U>>> {
        // Used by subscribers to send updates to the redis task.
        let (sender, outbound) = mpsc::channel(self.capacity);

        // Used by subscribers to receive updates from the redis task.
        let (fanout, receiver) = broadcast::channel(1);

        // Create a pubsub backend powered by redis.
        //
        // We do this eagerly so the channels used as part of our public API
        // do not get confused with those used in the redis client task.
        let backend = Redis { sender, receiver };

        // Construct a signer key to sign messages.
        //
        // Signed message are stored as plaintext. However, they cannot be
        // forged or modified by a malicious subscriber.
        //
        // Encyrption of data-in-transit and data-at-rest is a concern of
        // the redis deployment.
        //
        // We do our best to be responsible about plaintext residency while
        // remaining compliant with the privacy laws in the United States.
        //
        // If you are looking to secure your redis pubsub backend, the most
        // you can do without risking an audit is connecting to redis over
        // TLS.
        let signer = {
            // Confirm that a schema version was provided.
            let version = self.version.ok_or_else(|| require_argument("version"))?;

            // Confirm that the a valid signing key was provided.
            let signing_key = self
                .signing_key?
                .ok_or_else(|| require_argument("signing_key"))?;

            // Create a signer with the singing key and namespaced scope.
            Signer::new(signing_key, format!("via-pubsub:{}:v{}", scope, version))
        };

        // Spawn a detached task to process dispatch messages.
        tokio::spawn({
            // Create the redis client and establish a connection.
            let (mut redis, inbound) = connect(url).await?;

            // Subscribe to the update topic.
            redis.subscribe(signer.scope()).await?;

            // Create a dispatcher with the channel deps of the redis task.
            let trx = Dispatcher {
                fanout,
                inbound,
                outbound,
            };

            // Pin the task on the heap. It should live as long as the process.
            Box::pin(async {
                // Start receiving updates from peers.
                dispatch(redis, trx, signer).await;
            })
        });

        Ok(Pubsub::new(backend))
    }

    pub fn signing_key(mut self, bytes: &[u8]) -> Self {
        self.signing_key = Key::new(bytes).map(Some);
        self
    }

    pub fn version(mut self, version: u32) -> Self {
        self.version = Some(version);
        self
    }
}
