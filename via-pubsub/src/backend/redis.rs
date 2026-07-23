use redis::aio::{ConnectionManager, ConnectionManagerConfig, SendError};
use redis::{Client, PushInfo, PushKind, RedisResult, Value};
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

pub struct Builder<'a, T, U> {
    concurrency: Option<usize>,
    signing_key: Result<Option<Key>, Error>,
    sub_scope: &'a str,
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
    concurrency: usize,
}

async fn connect(
    concurrency: usize,
    url: &str,
) -> RedisResult<(ConnectionManager, mpsc::Receiver<PushInfo>)> {
    // The channel used internally by the redis client.
    //
    // A send error means that the recveiver was dropped or the subscriber
    // cannot preserve stream continuity.
    let (tx, pushes) = mpsc::channel(concurrency);

    let config = ConnectionManagerConfig::new()
        .set_pipeline_buffer_size(1)
        .set_concurrency_limit(1) // Implemented w/ pipelining.
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
    let mut send_buf = Vec::with_capacity(trx.concurrency);
    let mut recv_buf = Vec::with_capacity(trx.concurrency);

    loop {
        let send_offset = send_buf.len();
        let recv_offset = recv_buf.len();

        tokio::select! {
            // subscriber <- dispatch <- redis
            len @ 1.. = trx.inbound.recv_many(&mut recv_buf, trx.concurrency) => {
                let Some(buf) = recv_buf.get(recv_offset..recv_offset + len) else {
                    recv_buf = Vec::with_capacity(trx.concurrency);
                    continue;
                };

                log!(info, "batch size = {}", len);

                for payload in buf.iter().filter_map(|push| {
                    let scope = signer.scope();
                    extract_payload(push, scope)
                }) {
                    match signer.deserialize::<_, U>(payload) {
                        // Event deserialized successfully.
                        Ok(raw_peer_event) => {
                            if trx.fanout.send(raw_peer_event).is_err() {
                                return; // Receivers dropped. Don't become a zombie.
                            }
                        }

                        // Deserialization failed.
                        Err(ref error) => {
                            #[cfg(not(debug_assertions))]
                            let _ = error; // Placeholder for tracing...
                            log!(error, "{}", error);
                        }
                    }
                }

                recv_buf.clear();
            }

            // subscriber -> dispatch -> redis
            len @ 1.. = trx.outbound.recv_many(&mut send_buf, trx.concurrency) => {
                let mut pipeline = redis::pipe();
                let Some(buf) = send_buf.get(send_offset..send_offset + len) else {
                    // Placeholder for tracing...
                    send_buf = Vec::with_capacity(trx.concurrency);
                    continue;
                };

                log!(info, "pipeline size = {}", len);

                // Pack the batch of messages into the pipeline.
                buf.iter().fold(&mut pipeline, |pipe, event| {
                    match signer.serialize(&event) {
                        Ok((channel, payload)) => pipe
                            .cmd("PUBLISH")
                            .arg(channel)
                            .arg(payload),

                        Err(error) => {
                            #[cfg(not(debug_assertions))]
                            let _ = error; // Placeholder for tracing...
                            log!(error, "{}", &error);
                            pipe
                        }
                    }
                });

                // Publish to peers. If an error occurs, log it in debug builds.
                if let Err(error) = pipeline.exec_async(&mut redis).await {
                    #[cfg(not(debug_assertions))]
                    let _ = error; // Placeholder for tracing...
                    log!(error, "{}", &error);
                }

                send_buf.clear();
            }
        }
    }
}

#[inline]
fn extract_payload<'a>(push: &'a PushInfo, scope: &str) -> Option<&'a [u8]> {
    if let PushKind::Message = &push.kind
        && let Some([Value::BulkString(name), Value::BulkString(vec)]) =
            push.data.split_first_chunk().map(|(head, _)| head)
        && str::from_utf8(name).is_ok_and(|utf8| scope == utf8)
    {
        Some(vec.as_ref())
    } else {
        None
    }
}

fn require_argument(arg: &str) -> Error {
    Error::new(format!("missing required argument: \"{}\"", arg))
}

impl<T, U> Redis<T, U> {
    pub fn builder(scope: &str) -> Builder<'_, T, U> {
        Builder {
            concurrency: None,
            signing_key: Ok(None),
            sub_scope: scope,
            version: None,
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

impl<'a, T, U> Builder<'a, T, U>
where
    T: Copy + Eq + Hash + DeserializeOwned + Serialize + Send + Sync + 'static,
    U: DeserializeOwned + Serialize + Send + Sync + 'static,
{
    pub async fn connect(self, url: &str) -> via::Result<Pubsub<Redis<T, U>>> {
        // Confirm that `concurrency` was provided.
        let concurrency = self
            .concurrency
            .ok_or_else(|| require_argument("concurrency"))?;

        // Used by subscribers to send updates to the redis task.
        let (sender, outbound) = mpsc::channel(concurrency);

        // Used by subscribers to receive updates from the redis task.
        let (fanout, receiver) = broadcast::channel(concurrency);

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
            Signer::new(
                signing_key,
                format!("via-pubsub:{}:v{}", self.sub_scope, version),
            )
        };

        // Spawn a detached task to process dispatch messages.
        tokio::spawn({
            // Create the redis client and establish a connection.
            let (mut redis, inbound) = connect(concurrency, url).await?;

            // Subscribe to the update topic.
            redis.subscribe(signer.scope()).await?;

            // Create a dispatcher with the channel deps of the redis task.
            let trx = Dispatcher {
                fanout,
                inbound,
                outbound,
                concurrency,
            };

            // Pin the task on the heap. It should live as long as the process.
            Box::pin(async move {
                // Start receiving updates from peers.
                dispatch(redis, trx, signer).await;
            })
        });

        Ok(Pubsub::new(backend))
    }

    /// The number of events that can be published simultaneously.
    ///
    /// We suggest setting this value to the runtime worker count in order to
    /// uphold the following invariants:
    ///
    /// - No one should ever have to yield to publish
    /// - Subscription lag is indicative of app code that performs too much
    ///   work asynchronously without receiving a peer event
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::{env, thread};
    /// use via_pubsub::backend::Redis;
    /// # async fn build() -> via::Result<via_pubsub::Pubsub<Redis<(), ()>>> {
    /// Redis::builder("unicorn")
    ///     .concurrency(thread::available_parallelism()?.get())
    ///     //           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    ///     // The number of async tasks that can wake at once.
    ///     .signing_key(env::var("PUBSUB_SECRET")?.as_bytes())
    ///     .version(1)
    ///     .connect("redis://localhost:6379/?protocol=resp3")
    ///     .await
    /// # }
    /// ```
    pub fn concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
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
