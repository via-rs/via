use hyper::upgrade::Parts;
use std::fmt::{self, Display, Formatter};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};

use super::{CancellationToken, ServiceRequest};
use crate::server::IoStream;
use crate::server::cancel::catch_unwind;
use crate::{Error, Request};

#[derive(Debug)]
struct UpgradeError;

/// An upgraded HTTP connection.
pub struct Upgraded(hyper::upgrade::Upgraded);

/// Stop accepting connections and exit if a panic in a supervised future.
pub struct Supervisor {
    stream: Upgraded,
    waiter: CancellationToken,
}

struct OnUpgrade {
    on_upgrade: hyper::upgrade::OnUpgrade,
    supervisor: Arc<Mutex<Option<CancellationToken>>>,
}

pub async fn upgrade<App>(request: &mut Request<App>) -> Result<Supervisor, Error> {
    let supervisor = request
        .extensions_mut()
        .remove::<OnUpgrade>()
        .ok_or(UpgradeError)?;

    supervisor.upgrade().await
}

pub(super) fn upgradeable(request: &mut ServiceRequest, waiter: CancellationToken) {
    let extensions = request.extensions_mut();

    if let Some(on_upgrade) = extensions.remove::<hyper::upgrade::OnUpgrade>() {
        let supervisor = Arc::new(Mutex::new(Some(waiter)));

        extensions.insert(OnUpgrade {
            on_upgrade,
            supervisor,
        });
    }
}

impl Supervisor {
    pub async fn handshake<F, Await, Supervise>(
        self,
        f: F,
    ) -> Result<impl Future<Output = ()> + Send + 'static, Error>
    where
        F: FnOnce(Upgraded) -> Await,
        Await: Future<Output = Result<Supervise, Error>>,
        Supervise: Future<Output = ()> + Send + 'static,
    {
        let future = f(self.stream).await?;
        Ok(catch_unwind(future, self.waiter))
    }
}

impl Upgraded {
    pub fn downcast(self) -> Result<Parts<impl AsyncRead + AsyncWrite + Send + Unpin>, Error> {
        self.0
            .downcast::<IoStream>()
            .map_err(|_| UpgradeError.into())
    }
}

impl OnUpgrade {
    async fn upgrade(self) -> Result<Supervisor, Error> {
        let stream = Upgraded(self.on_upgrade.await?);
        let waiter = match self.supervisor.try_lock() {
            Ok(mut guard) => guard.take().ok_or(UpgradeError)?,
            Err(_) => return Err(UpgradeError.into()),
        };

        Ok(Supervisor { stream, waiter })
    }
}

impl Clone for OnUpgrade {
    fn clone(&self) -> Self {
        Self {
            on_upgrade: self.on_upgrade.clone(),
            supervisor: Arc::new(Mutex::new(None)),
        }
    }
}

impl std::error::Error for UpgradeError {}

impl Display for UpgradeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "an unknown error occured during the upgrade handshake.")
    }
}
