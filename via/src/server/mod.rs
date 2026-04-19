//! Serve an [App](crate::App) over HTTP or HTTPS.
//!

mod accept;
mod cancel;
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

#[cfg(feature = "rustls-23")]
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
    http1_header_read_timeout: Duration,

    #[cfg(any(feature = "native-tls", feature = "rustls-23"))]
    tls_handshake_timeout: Duration,

    #[cfg(any(feature = "native-tls", feature = "rustls-23"))]
    http2_max_concurrent_streams: Option<u32>,

    #[cfg(any(feature = "native-tls", feature = "rustls-23"))]
    http2_max_send_buf_size: usize,
}

impl<App> Server<App>
where
    App: Send + Sync + 'static,
{
    /// Creates a new server for the provided app.
    pub fn new(app: Via<App>) -> Self {
        Self {
            app,
            config: Default::default(),
        }
    }

    /// Enables or disables HTTP/1.1 persistent connections.
    ///
    /// When enabled, the server will allow clients to reuse a TCP connection
    /// for multiple requests. Disabling keep-alive forces the connection to be
    /// closed after each response is sent.
    ///
    /// Disabling keep-alive may reduce resource retention from idle clients but
    /// can increase connection overhead due to additional TCP and TLS handshakes.
    ///
    /// **Default:** `true`
    pub fn keep_alive(mut self, keep_alive: bool) -> Self {
        self.config.keep_alive = keep_alive;
        self
    }

    /// Sets the maximum size of the HTTP/1.1 connection read buffer.
    ///
    /// This buffer is used when reading and parsing the HTTP request line and
    /// headers from the client. It does **not** limit the size of the request
    /// body. Use [`Server::max_request_size`] to limit the maximum allowed
    /// request body size.
    ///
    /// **Default:** `16 KB`
    pub fn max_buf_size(mut self, max_buf_size: usize) -> Self {
        self.config.max_buf_size = max_buf_size;
        self
    }

    /// Sets the maximum number of concurrent connections that the server can
    /// accept.
    ///
    /// **Default:** `1000`
    pub fn max_connections(mut self, max_connections: usize) -> Self {
        self.config.max_connections = max_connections;
        self
    }

    /// Set the maximum request body size in bytes.
    ///
    /// **Default:** `100 MB`
    pub fn max_request_size(mut self, max_request_size: usize) -> Self {
        self.config.max_request_size = max_request_size;
        self
    }

    /// Sets the amount of time that the server will wait for inflight
    /// connections to complete before shutting down.
    ///
    /// **Default:** `10s`
    pub fn shutdown_timeout(mut self, shutdown_timeout: Duration) -> Self {
        self.config.shutdown_timeout = shutdown_timeout;
        self
    }

    /// If a client does not transmit the entire header within this time, the
    /// connection is closed.
    ///
    /// **Default:** `10s`
    /// **Max:** `30s`
    pub fn http1_header_read_timeout(mut self, http1_header_read_timeout: Duration) -> Self {
        self.config.http1_header_read_timeout = http1_header_read_timeout;
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

    #[cfg(feature = "rustls-23")]
    pub async fn listen_rustls_23(
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

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
impl<App> Server<App> {
    /// Sets the maximum number of concurrent HTTP/2 streams allowed per
    /// connection.
    ///
    /// Each stream represents an independent request/response exchange.
    /// Limiting the number of concurrent streams helps bound per-connection
    /// resource usage and reduces the impact of abusive clients.
    ///
    /// A `None` value removes the limit entirely.
    ///
    /// **Default:** `Some(64)`
    ///
    pub fn http2_max_concurrent_streams(mut self, max_concurrent_streams: Option<u32>) -> Self {
        self.config.http2_max_concurrent_streams = max_concurrent_streams;
        self
    }

    /// Sets the maximum size of the internal HTTP/2 send buffer used for
    /// buffering outbound frames before they are written to the underlying
    /// transport.
    ///
    /// Larger buffers may improve throughput for high-latency networks but
    /// increase per-connection memory usage.
    ///
    /// Smaller buffers reduce memory usage and improve backpressure behavior
    /// but may increase the number of write operations.
    ///
    /// **Default:** `64 KB`
    ///
    pub fn http2_max_send_buf_size(mut self, max_send_buf_size: usize) -> Self {
        self.config.http2_max_send_buf_size = max_send_buf_size;
        self
    }

    /// The amount of time in seconds that an individual connection task will
    /// wait for the TLS handshake to complete before closing the connection.
    ///
    /// This configuration is only used when a TLS backend is enabled.
    ///
    /// **Default:** `5s`
    ///
    pub fn tls_handshake_timeout(mut self, tls_handshake_timeout: Duration) -> Self {
        if tls_handshake_timeout.is_zero() {
            panic!("tls_handshake_timeout must be > 0");
        }

        self.config.tls_handshake_timeout = tls_handshake_timeout;
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

    pub fn http1_header_read_timeout(&self) -> Duration {
        self.http1_header_read_timeout.min(Duration::from_secs(30))
    }
}

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
impl ServerConfig {
    pub fn http2_max_concurrent_streams(&self) -> Option<u32> {
        self.http2_max_concurrent_streams
    }

    pub fn http2_max_send_buf_size(&self) -> usize {
        self.http2_max_send_buf_size
    }

    pub fn tls_handshake_timeout(&self) -> Duration {
        self.tls_handshake_timeout
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
            http1_header_read_timeout: Duration::from_secs(10),

            #[cfg(any(feature = "native-tls", feature = "rustls-23"))]
            http2_max_concurrent_streams: Some(64),

            #[cfg(any(feature = "native-tls", feature = "rustls-23"))]
            http2_max_send_buf_size: 65536, // 64 KB

            #[cfg(any(feature = "native-tls", feature = "rustls-23"))]
            tls_handshake_timeout: Duration::from_secs(5),
        }
    }
}
