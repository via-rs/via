use serde::{Deserialize, Serialize};
use std::path::Path;
use std::{fs, io};
use tokio::sync::{mpsc, oneshot};
use via::error::{BoxError, Error};

use super::models::*;
use super::table::{Id, Table};

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");
const SCHEMA_FROM_EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/chat/schema.json");
const SCHEMA_FROM_WORKSPACE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/examples/chat/schema.json");

pub struct Database {
    tx: mpsc::Sender<Command>,
}

enum Command {
    FindChannel {
        tx: oneshot::Sender<Option<Channel>>,
        id: Id,
    },
    FindSubscription {
        tx: oneshot::Sender<Option<Subscription>>,
        id: Id,
    },
    FindUser {
        tx: oneshot::Sender<Option<User>>,
        id: Id,
    },
    FetchUserByUsername {
        tx: oneshot::Sender<Option<User>>,
        username: String,
    },

    InsertChannel {
        tx: oneshot::Sender<Result<Channel, BoxError>>,
        payload: NewChannel,
    },
    InsertSubscription {
        tx: oneshot::Sender<Result<Subscription, BoxError>>,
        payload: NewSubscription,
    },
    InsertUser {
        tx: oneshot::Sender<Result<User, BoxError>>,
        payload: NewUser,
    },
}

#[derive(Deserialize, Serialize)]
struct Schema {
    channels: Table<Channel>,
    subscriptions: Table<Subscription>,
    users: Table<User>,
}

fn warn_receiver_dropped<T>(_: &T) {
    if cfg!(debug_assertions) {
        eprintln!("warn(database): sender dropped");
    }
}

async fn recv_loop(mut schema: Schema, mut rx: mpsc::Receiver<Command>) {
    while let Some(command) = rx.recv().await {
        match command {
            Command::FindChannel { tx, id } => {
                let channel_opt = schema.channels.get(&id);
                let _ = tx.send(channel_opt).inspect_err(warn_receiver_dropped);
            }
            Command::FindSubscription { tx, id } => {
                let subscription_opt = schema.subscriptions.get(&id);
                let _ = tx.send(subscription_opt).inspect_err(warn_receiver_dropped);
            }
            Command::FindUser { tx, id } => {
                let user_opt = schema.users.get(&id);
                let _ = tx.send(user_opt).inspect_err(warn_receiver_dropped);
            }
            Command::FetchUserByUsername { tx, username } => {
                let user_opt = schema
                    .users
                    .iter()
                    .find(|user| username == user.username())
                    .cloned();

                let _ = tx.send(user_opt).inspect_err(warn_receiver_dropped);
            }

            Command::InsertChannel { tx, payload } => {
                let insert_channel = schema.channels.insert(payload);
                let _ = tx.send(insert_channel).inspect_err(warn_receiver_dropped);
            }
            Command::InsertSubscription { tx, payload } => {
                let insert_subscription = schema.subscriptions.insert(payload);
                let _ = tx
                    .send(insert_subscription)
                    .inspect_err(warn_receiver_dropped);
            }
            Command::InsertUser { tx, payload } => {
                let insert_user = schema.users.insert(payload);
                let _ = tx.send(insert_user).inspect_err(warn_receiver_dropped);
            }
        }
    }
}

impl Database {
    pub fn new() -> Result<Self, Error> {
        let (tx, rx) = mpsc::channel(1024);
        let schema = Schema::load(if CARGO_MANIFEST_DIR.ends_with("examples") {
            SCHEMA_FROM_EXAMPLES
        } else {
            SCHEMA_FROM_WORKSPACE
        })?;

        tokio::spawn(Box::pin(recv_loop(schema, rx)));

        Ok(Self { tx })
    }

    pub async fn find_user(&self, id: Id) -> Result<Option<User>, Error> {
        let (tx, rx) = oneshot::channel();

        self.tx.send(Command::FindUser { tx, id }).await?;

        Ok(rx.await?)
    }

    pub async fn fetch_user_by_username(&self, username: String) -> Result<Option<User>, Error> {
        let (tx, rx) = oneshot::channel();

        self.tx
            .send(Command::FetchUserByUsername { tx, username })
            .await?;

        Ok(rx.await?)
    }
}

impl Schema {
    fn new() -> Self {
        Self {
            channels: Table::new(),
            subscriptions: Table::new(),
            users: Table::new(),
        }
    }

    fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();

        match fs::read(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.into()),
            Ok(json) => serde_json::from_slice(&json).map_err(|error| error.into()),
            _ => {
                let schema = Self::new();
                let json = serde_json::to_vec_pretty(&schema)?;

                fs::write(path, json)?;
                Ok(schema)
            }
        }
    }
}
