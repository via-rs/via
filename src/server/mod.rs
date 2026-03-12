//! Serve an [App](crate::App) over HTTP or HTTPS.
//!

mod accept;
mod io;
mod tls;

use std::process::ExitCode;
use std::time::Duration;
use tokio::net::{TcpListener, ToSocketAddrs};

use crate::app::{ServiceAdapter, Via};
use crate::error::Error;
use accept::accept;
use io::IoWithPermit;
use tls::TcpAcceptor;

#[cfg(feature = "native-tls")]
use tls::NativeTlsAcceptor;

#[cfg(feature = "rustls")]
use tls::RustlsAcceptor;

/// Serve an app over HTTP.
///
pub struct Server<App> {
    app: Via<App>,
    config: ServerConfig,
}

#[derive(Debug)]
pub(crate) struct ServerConfig {
    keep_alive: bool,
    max_buf_size: usize,
    max_connections: usize,
    max_request_size: usize,
    shutdown_timeout: Duration,

    #[cfg(any(feature = "native-tls", feature = "rustls"))]
    handshake_timeout: Duration,

    #[cfg(any(feature = "native-tls", feature = "rustls"))]
    http2_max_concurrent_streams: Option<u32>,

    #[cfg(any(feature = "native-tls", feature = "rustls"))]
    http2_max_send_buf_size: usize,
}

impl<App> Server<App>
where
    App: Send + Sync + 'static,
{
    /// Creates a new server for the provided app.
    ///
    pub fn new(app: Via<App>) -> Self {
        Self {
            app,
            config: Default::default(),
        }
    }

    pub fn keep_alive(mut self, keep_alive: bool) -> Self {
        self.config.keep_alive = keep_alive;
        self
    }

    /// **Default:** `16 KB`
    ///
    pub fn max_buf_size(mut self, max_buf_size: usize) -> Self {
        self.config.max_buf_size = max_buf_size;
        self
    }

    /// Sets the maximum number of concurrent connections that the server can
    /// accept.
    ///
    /// **Default:** `1000`
    ///
    pub fn max_connections(mut self, max_connections: usize) -> Self {
        self.config.max_connections = max_connections;
        self
    }

    /// Set the maximum request body size in bytes.
    ///
    /// **Default:** `100 MB`
    ///
    pub fn max_request_size(mut self, max_request_size: usize) -> Self {
        self.config.max_request_size = max_request_size;
        self
    }

    /// Set the amount of time in seconds that the server will wait for inflight
    /// connections to complete before shutting down.
    ///
    /// **Default:** `10s`
    pub fn shutdown_timeout(mut self, shutdown_timeout: Duration) -> Self {
        self.config.shutdown_timeout = shutdown_timeout;
        self
    }

    /// Listens for incoming connections at the provided address.
    ///
    /// Returns a future that resolves with a result containing an [`ExitCode`]
    /// when shutdown is requested.
    ///
    /// # Errors
    ///
    /// - If the server fails to bind to the provided address.
    /// - If the `rustls` feature is enabled and `rustls_config` is missing.
    ///
    /// # Exit Codes
    ///
    /// An [`ExitCode::SUCCESS`] can be viewed as a confirmation that every
    /// request was served before exiting the accept loop.
    ///
    /// An [`ExitCode::FAILURE`] is an indicator that an unrecoverable error
    /// occured which requires that the server be restarted in order to function
    /// as intended.
    ///
    /// If you are running your Via application as a daemon with a process
    /// supervisor such as upstart or systemd, you can use the exit code to
    /// determine whether or not the process should restart.
    ///
    /// If you are running your Via application in a cluster behind a load
    /// balancer you can use the exit code to properly configure node replacement
    /// and / or decommissioning logic.
    ///
    /// When high availability is mission-critical, and you are scaling your Via
    /// application both horizontally and vertically using a combination of the
    /// aforementioned deployment strategies, we recommend configuring a temporal
    /// threshold for the number of restarts caused by an [`ExitCode::FAILURE`].
    /// If the threshold is exceeded the cluster should immutably replace the
    /// node and the process supervisor should not make further attempts to
    /// restart the process.
    ///
    /// This approach significantly reduces the impact of environmental entropy
    /// on your application's availability while preventing conflicts between the
    /// process supervisor of an individual node and the replacement and
    /// decommissioning logic of the cluster.
    pub async fn listen(self, address: impl ToSocketAddrs) -> Result<ExitCode, Error> {
        let future = accept(
            TcpAcceptor,
            TcpListener::bind(address).await?,
            ServiceAdapter::new(self.config, self.app),
        );

        Ok(future.await)
    }

    #[cfg(feature = "native-tls")]
    pub async fn listen_native_tls(
        self,
        address: impl ToSocketAddrs,
        identity: native_tls::Identity,
    ) -> Result<ExitCode, Error> {
        let future = accept(
            NativeTlsAcceptor::new(identity),
            TcpListener::bind(address).await?,
            ServiceAdapter::new(self.config, self.app),
        );

        Ok(future.await)
    }

    #[cfg(feature = "rustls")]
    pub async fn listen_rustls(
        self,
        address: impl ToSocketAddrs,
        rustls_config: rustls::ServerConfig,
    ) -> Result<ExitCode, Error> {
        let future = accept(
            RustlsAcceptor::new(rustls_config),
            TcpListener::bind(address).await?,
            ServiceAdapter::new(self.config, self.app),
        );

        Ok(future.await)
    }
}

#[cfg(any(feature = "native-tls", feature = "rustls"))]
impl<App> Server<App> {
    /// The amount of time in seconds that an individual connection task will
    /// wait for the TLS handshake to complete before closing the connection.
    ///
    /// This configuration is only used when a TLS backend is enabled.
    ///
    /// **Default:** `5s`
    ///
    pub fn handshake_timeout(mut self, handshake_timeout: Duration) -> Self {
        if handshake_timeout.is_zero() {
            panic!("handshake_timeout must be > 0");
        }

        self.config.handshake_timeout = handshake_timeout;
        self
    }

    pub fn http2_max_concurrent_streams(mut self, max_concurrent_streams: Option<u32>) -> Self {
        self.config.http2_max_concurrent_streams = max_concurrent_streams;
        self
    }

    pub fn http2_max_send_buf_size(mut self, max_send_buf_size: usize) -> Self {
        self.config.http2_max_send_buf_size = max_send_buf_size;
        self
    }
}

impl ServerConfig {
    pub fn keep_alive(&self) -> bool {
        self.keep_alive
    }

    pub fn max_buf_size(&self) -> usize {
        self.max_buf_size
    }

    pub fn max_connections(&self) -> usize {
        self.max_connections
    }

    pub fn max_request_size(&self) -> usize {
        self.max_request_size
    }

    pub fn shutdown_timeout(&self) -> Duration {
        self.shutdown_timeout
    }
}

#[cfg(any(feature = "native-tls", feature = "rustls"))]
impl ServerConfig {
    pub fn http2_max_concurrent_streams(&self) -> Option<u32> {
        self.http2_max_concurrent_streams
    }

    pub fn http2_max_send_buf_size(&self) -> usize {
        self.http2_max_send_buf_size
    }

    pub fn tls_handshake_timeout(&self) -> Duration {
        self.handshake_timeout
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            keep_alive: true,
            max_buf_size: 16384, // 16 KB
            max_connections: 1000,
            max_request_size: 104_857_600, // 100 MB
            shutdown_timeout: Duration::from_secs(10),

            #[cfg(any(feature = "native-tls", feature = "rustls"))]
            handshake_timeout: Duration::from_secs(5),

            #[cfg(any(feature = "native-tls", feature = "rustls"))]
            http2_max_concurrent_streams: Some(64),

            #[cfg(any(feature = "native-tls", feature = "rustls"))]
            http2_max_send_buf_size: 65536, // 64 KB
        }
    }
}
