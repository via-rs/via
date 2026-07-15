use serde::{Serialize, de::DeserializeOwned};
use std::marker::PhantomData;
use tokio::sync::{broadcast, mpsc};
use via::Error;

use super::{Backend, Interest, Publish};
use crate::pubsub::{Receiver, Sender};

pub struct Builder<T, U> {
    url: String,
    name: Option<String>,
    version: Option<u32>,
    capacity: Option<usize>,
    _schema: PhantomData<U>,
    _interest: PhantomData<T>,
}

pub struct Redis<T, U> {
    url: String,
    topic: String,
    capacity: usize,
    _schema: PhantomData<U>,
    _interest: PhantomData<T>,
}

fn require_argument(arg: &str) -> Error {
    Error::new(format!("missing required argument: \"{}\"", arg))
}

impl<T, U> Redis<T, U> {
    pub fn builder(url: &str) -> Builder<T, U> {
        Builder {
            url: url.to_owned(),
            name: None,
            version: None,
            capacity: None,
            _schema: PhantomData,
            _interest: PhantomData,
        }
    }
}

impl<T, U> Backend<T, U> for Redis<T, U>
where
    T: Interest + Send + Sync + 'static,
    U: DeserializeOwned + Serialize + Send + Sync + 'static,
{
    async fn connect(&self) -> via::Result<(Sender<T, U>, Receiver<T>)> {
        let (tx, task_rx) = mpsc::channel::<Publish<T, U>>(self.capacity);
        let (task_tx, rx) = broadcast::channel(self.capacity);

        tokio::spawn({
            let _tx = task_tx;
            let mut rx = task_rx;

            Box::pin(async move {
                // TODO:
                // Deserialize message we receive from redis as Publish<T, U>
                // to confirm that it's validity once and then convert it to
                // an event that can be cloned without copying it's payload.
                while let Some(event) = rx.recv().await {
                    match serde_json::to_string_pretty(&event) {
                        Ok(publish) => {
                            println!("publish: {:#?}", publish)
                        }
                        Err(error) => {
                            eprintln!("{}", error);
                        }
                    }
                }
            })
        });

        Ok((tx, rx))
    }
}

impl<T, U> Builder<T, U> {
    pub fn build(self) -> via::Result<Redis<T, U>> {
        let url = self.url;
        let name = self.name.as_ref().ok_or_else(|| require_argument("name"))?;
        let version = self.version.ok_or_else(|| require_argument("version"))?;
        let capacity = self.capacity.ok_or_else(|| require_argument("capacity"))?;

        Ok(Redis {
            url,
            capacity,
            topic: format!("via-pubsub:{}:{}", name, version),
            _schema: PhantomData,
            _interest: PhantomData,
        })
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_owned());
        self
    }

    pub fn version(mut self, version: u32) -> Self {
        self.version = Some(version);
        self
    }

    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = Some(capacity);
        self
    }
}
