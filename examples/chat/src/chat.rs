use bb8::Pool;
use bb8::{ManageConnection, PooledConnection, RunError};
use bytes::Bytes;
use cookie::Key;
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use http::header;
use serde::Serialize;
use std::env::{self, VarError};
use tokio::sync::broadcast;
use tokio::task::coop::unconstrained;
use via::response::{Finalize, ResponseBuilder};
use via::ws::Utf8Bytes;
use via::{raise, ws};

use crate::models::conversation::Conversation;
use crate::models::reaction::Reaction;
use crate::util::Id;

pub type Connection<'a> = PooledConnection<'a, ConnectionManager>;
pub type ConnectionError = RunError<<ConnectionManager as ManageConnection>::Error>;
pub type ConnectionManager = AsyncDieselConnectionManager<AsyncPgConnection>;

type Sender = broadcast::Sender<(EventContext, EventPayload)>;
type Receiver = broadcast::Receiver<(EventContext, EventPayload)>;

#[derive(Clone)]
pub struct EventPayload(Utf8Bytes);

#[derive(Serialize)]
#[serde(content = "data", rename_all = "lowercase", tag = "type")]
pub enum Event {
    Message(Conversation),
    Reaction(Reaction),
}

pub struct Chat {
    database: Pool<ConnectionManager>,
    channel: (Sender, Receiver),
    secret: Key,
}

#[derive(Clone, Debug)]
pub struct EventContext {
    channel_id: Option<Id>,
    user_id: Id,
}

pub async fn establish_pg_connection() -> Pool<ConnectionManager> {
    let database_url = require_env("DATABASE_URL");
    let manager = ConnectionManager::new(&database_url);
    let result = Pool::builder().build(manager).await;

    result.unwrap_or_else(|error| {
        panic!(
            "failed to establish database connection: url = {}, error = {}",
            database_url, error
        );
    })
}

pub fn load_session_secret() -> Key {
    let secret = require_env("VIA_SECRET_KEY");
    let result = secret.as_bytes().try_into();

    result.expect("unexpected end of input while parsing VIA_SECRET_KEY")
}

fn require_env(var: &str) -> String {
    env::var(var).unwrap_or_else(|error| match error {
        VarError::NotPresent => panic!("missing required env var: {}", var),
        VarError::NotUnicode(_) => panic!("env var \"{}\" is not valid UTF-8", var),
    })
}

impl Chat {
    pub fn new(database: Pool<ConnectionManager>, secret: Key) -> Self {
        let channel = broadcast::channel(1024);

        Self {
            database,
            channel,
            secret,
        }
    }

    pub fn database(&self) -> impl Future<Output = Result<Connection<'_>, ConnectionError>> {
        // Acquire a database connection without consuming Tokio’s cooperative
        // scheduling budget.
        //
        // We know that immediately after the future returned from this fn is
        // ready, we will have to wait on network I/O from the database. The
        // I/O performed over the connection will naturally yield back to the
        // scheduler, giving other async tasks an opportunity to run.
        unconstrained(self.database.get())
    }

    pub fn publish(&self, context: EventContext, event: Event) -> via::Result<EventPayload> {
        let payload = EventPayload(serde_json::to_string(&event)?.into());

        if self.channel.0.send((context, payload.clone())).is_err() {
            raise!(message = "pubsub channel closed.");
        }

        Ok(payload)
    }

    pub fn secret(&self) -> &Key {
        &self.secret
    }

    pub fn subscribe(&self) -> Receiver {
        self.channel.0.subscribe()
    }
}

impl Finalize for EventPayload {
    fn finalize(self, builder: ResponseBuilder) -> via::Result {
        let bytes = Bytes::copy_from_slice(self.0.as_bytes());

        builder
            .header(header::CONTENT_LENGTH, bytes.len())
            .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .body(bytes.into())
    }
}

impl From<EventPayload> for ws::Message {
    fn from(payload: EventPayload) -> Self {
        ws::Message::Text(payload.0)
    }
}

impl EventContext {
    pub fn new(channel_id: Option<Id>, user_id: Id) -> Self {
        Self {
            channel_id,
            user_id,
        }
    }

    pub fn channel_id(&self) -> Option<&Id> {
        self.channel_id.as_ref()
    }

    pub fn user_id(&self) -> &Id {
        &self.user_id
    }
}
