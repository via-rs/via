mod dispatch;
mod sub;

use redis::aio::{ConnectionManager, ConnectionManagerConfig, SendError};
use redis::{Client, PushInfo, RedisResult};
use serde::{Serialize, de::DeserializeOwned};
use std::hash::Hash;
use std::marker::PhantomData;
use tokio::sync::{broadcast, mpsc};
use via::error::Error;

use crate::pubsub::Dispatch;
use crate::redis::dispatch::dispatch;
use crate::scope::Filter;
use crate::sign::{InvalidKeyError, Key, OurEvent, RawPeerEvent, Signer};

use dispatch::Dispatcher;
use sub::{Channel, Subscription};

/// The maximum size in bytes of a pipeline command.
const MAX_PIPELINE_SIZE: usize = 65536;

pub struct Builder<'a, T, U> {
    concurrency: Option<usize>,
    max_event_size: Option<usize>,
    signing_key: Result<Option<Key>, InvalidKeyError>,
    namespace: &'a str,
    version: Option<u32>,
    _ty: PhantomData<(T, U)>,
}

pub struct Redis<T, U> {
    sender: mpsc::Sender<OurEvent<T, U>>,
    fanout: broadcast::Sender<RawPeerEvent<T>>,
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

fn require_argument(arg: &str) -> Error {
    Error::new(format!("missing required argument: \"{}\"", arg))
}

impl<'a, T, U> Builder<'a, T, U>
where
    T: Copy + Eq + Hash + DeserializeOwned + Serialize + Send + Sync + 'static,
    U: DeserializeOwned + Serialize + Send + Sync + 'static,
{
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

    /// The maximum size event size in bytes of a serialized event.
    ///
    /// The provided value is treated as an upper bound to determine a
    /// pathological publish burst.
    ///
    /// This allows us to calculate the number of events that can be safely
    /// included in a pipeline request to redis without keeping a running total
    /// of the size in bytes of the command.
    pub fn max_event_size(mut self, max: usize) -> Self {
        self.max_event_size = Some(max);
        self
    }

    pub fn signing_key(mut self, bytes: impl AsRef<[u8]>) -> Self {
        self.signing_key = Key::new(bytes).map(Some);
        self
    }

    pub fn version(mut self, version: u32) -> Self {
        self.version = Some(version);
        self
    }

    pub async fn connect(self, url: &str) -> via::Result<Redis<T, U>> {
        // Confirm that `concurrency` was provided.
        let concurrency = self
            .concurrency
            .ok_or_else(|| require_argument("concurrency"))?;

        // Confirm that `max_event_size` was provided.
        //
        // We use this to determine the maximum number of events that can be
        // safely included in a pipeline command to redis.
        let max_event_size = self
            .max_event_size
            .ok_or_else(|| require_argument("max_event_size"))?;

        // Used by subscribers to send updates to the redis task.
        let (sender, outbound) = mpsc::channel(concurrency);

        // Used by subscribers to receive updates from the redis task.
        let (fanout, _) = broadcast::channel(concurrency);

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
                format!("via-pubsub:{}:v{}", self.namespace, version),
            )
        };

        // Spawn a detached task to process dispatch messages.
        tokio::spawn({
            // Create the redis client and establish a connection.
            let (mut redis, inbound) = connect(concurrency, url).await?;

            // Subscribe to the update topic.
            redis.subscribe(signer.scope()).await?;

            // Create a dispatcher with the channel deps of the redis task.
            let dispatcher = Dispatcher {
                fanout: fanout.clone(),
                inbound,
                outbound,
                concurrency: MAX_PIPELINE_SIZE.div_euclid(max_event_size),
            };

            // Start receiving updates from peers.
            dispatch(redis, signer, dispatcher)
        });

        Ok(Redis { sender, fanout })
    }
}

impl<T, U> Redis<T, U> {
    pub fn builder(scope: &str) -> Builder<'_, T, U> {
        Builder {
            concurrency: None,
            max_event_size: None,
            signing_key: Ok(None),
            namespace: scope,
            version: None,
            _ty: PhantomData,
        }
    }
}

impl<T, U> Dispatch<T, U> for Redis<T, U>
where
    T: Clone + Eq + Hash + Send,
    U: DeserializeOwned + Serialize + Send,
{
    type Subscription = Subscription<T, U>;

    fn dispatch(&self, event: OurEvent<T, U>) {
        if self.sender.try_send(event).is_err() {
            log!(warn, "failed to synchronously send event.");
        }
    }

    fn subscribe(&self, actor: T) -> Self::Subscription {
        let sender = self.sender.clone();
        let receiver = self.fanout.subscribe();

        Subscription {
            scope: Filter::new(actor, Channel { sender, receiver }),
        }
    }
}
