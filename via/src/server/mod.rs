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
use crate::router::Router;

use accept::accept;
use tls::TcpAcceptor;

#[cfg(feature = "native-tls")]
use tls::NativeTlsAcceptor;

#[cfg(feature = "rustls-23")]
use tls::RustlsAcceptor;

#[cfg(all(
    any(feature = "tokio-tungstenite", feature = "tokio-websockets"),
    not(feature = "rustls-23"),
    feature = "native-tls",
))]
pub(crate) type IoStream = io::IoWithPermit<tls::NativeTlsStream>;

#[cfg(all(
    any(feature = "tokio-tungstenite", feature = "tokio-websockets"),
    not(feature = "native-tls"),
    feature = "rustls-23",
))]
pub(crate) type IoStream = io::IoWithPermit<tls::RustlsStream>;

#[cfg(all(
    any(feature = "tokio-tungstenite", feature = "tokio-websockets"),
    not(any(feature = "native-tls", feature = "rustls-23"))
))]
pub(crate) type IoStream = io::IoWithPermit<tokio::net::TcpStream>;

const DEFAULT_MAX_CONNECTIONS: usize = 1024;
const RUNTIME_FD_BUDGET: usize = 10;

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
    /// Create a new server for `router` and `app`.
    pub fn new(router: Router<App>, app: App) -> Self {
        Self {
            app: Via::new(router, app),
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
    /// Return whether HTTP keep-alive is enabled.
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
    /// Return the maximum per-connection buffer size.
    pub fn max_buf_size(mut self, max_buf_size: usize) -> Self {
        self.config.max_buf_size = max_buf_size;
        self
    }

    /// Sets the maximum number of concurrent connections that the server can
    /// accept.
    ///
    /// An idle Via application uses 10 file descriptors on POSIX platforms.
    ///
    /// Rather than making you do math for no reason, we subtract 10 from the
    /// provided connection budget.
    ///
    /// **Default:** `1024`
    pub fn max_connections(mut self, max_connections: usize) -> Self {
        assert!(
            max_connections > RUNTIME_FD_BUDGET,
            "max_connections must be > {}",
            RUNTIME_FD_BUDGET,
        );

        self.config.max_connections = max_connections - RUNTIME_FD_BUDGET;
        self
    }

    /// Reserve the exact number of file descriptors used in your application.
    ///
    /// Determinism is the backbone of high assurance.
    ///
    /// If you know the exact number of resources your application uses, set
    /// this value to ensure the exact amount of backpressure is applied in
    /// accept.
    ///
    /// # Panics
    ///
    /// Panics if `len` is >= to the maximum number of connections.
    pub fn reserve_file_descriptors(mut self, len: usize) -> Self {
        assert!(
            len < self.config.max_connections,
            "reserved fd len must be < max_connections ({})",
            self.config.max_connections,
        );

        self.config.max_connections -= len;
        self
    }

    /// Set the maximum request body size in bytes.
    ///
    /// **Default:** `100 MB`
    /// Return the maximum accepted request body size.
    pub fn max_request_size(mut self, max_request_size: usize) -> Self {
        self.config.max_request_size = max_request_size;
        self
    }

    /// Sets the amount of time that the server will wait for inflight
    /// connections to complete before shutting down.
    ///
    /// **Default:** `10s`
    /// **Max:** `30s`
    /// Return the graceful shutdown timeout, capped at 30 seconds.
    pub fn shutdown_timeout(mut self, duration: Duration) -> Self {
        self.config.shutdown_timeout = (duration <= Duration::from_secs(30))
            .then_some(duration)
            .expect("\"shutdown_timeout\" exceeds maximum value of 30 seconds.");

        self
    }

    /// If a client does not transmit the entire request head within this time,
    /// the connection is closed.
    ///
    /// **Default:** `10s`
    /// **Max:** `30s`
    /// Return the HTTP/1 header read timeout, capped at 30 seconds.
    pub fn http1_header_read_timeout(mut self, duration: Duration) -> Self {
        self.config.http1_header_read_timeout = (duration <= Duration::from_secs(30))
            .then_some(duration)
            .expect("\"http1_header_read_timeout\" exceeds maximum value of 30 seconds.");

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

    /// Listens for incoming HTTPS connections using `native-tls` for TLS
    /// termination.
    ///
    /// This variant enables TLS using a platform-native TLS implementation via
    /// the `native-tls` crate.
    ///
    /// It accepts a server identity (certificate + private key) and wraps
    /// incoming TCP connections in a TLS stream before handing them to the
    /// HTTP service layer.
    ///
    /// # Errors
    ///
    /// - If the server fails to bind to the provided address.
    /// - If the [`native_tls::Identity`] struct is invalid.
    ///
    /// # Exit Codes
    ///
    /// See [`Server::listen`] for details on exit code semantics.
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

    /// Listens for incoming HTTPS connections using `rustls` for TLS
    /// termination.
    ///
    /// This variant enables TLS using `rustls v0.23`, a pure-Rust TLS
    /// implementation.
    ///
    /// It is typically preferred in environments where deterministic behavior,
    /// portability, or avoidance of system TLS dependencies is desired.
    ///
    /// It accepts a server identity (certificate + private key) and wraps
    /// incoming TCP connections in a TLS stream before handing them to the
    /// HTTP service layer.
    ///
    /// # Errors
    ///
    /// - If the server fails to bind to the provided address.
    /// - If the [`rustls::ServerConfig`] struct is invalid.
    ///
    /// # Exit Codes
    ///
    /// See [`Server::listen`] for details on exit code semantics.
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
    /// Return the configured HTTP/2 max concurrent streams value.
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
    /// Return the configured HTTP/2 maximum send buffer size.
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
    /// Return the configured TLS handshake timeout.
    pub fn tls_handshake_timeout(mut self, tls_handshake_timeout: Duration) -> Self {
        if tls_handshake_timeout.is_zero() {
            panic!("tls_handshake_timeout must be > 0");
        }

        self.config.tls_handshake_timeout = tls_handshake_timeout;
        self
    }
}

impl ServerConfig {
    /// Return whether HTTP keep-alive is enabled.
    pub fn keep_alive(&self) -> bool {
        self.keep_alive
    }

    /// Return the maximum per-connection buffer size.
    pub fn max_buf_size(&self) -> usize {
        self.max_buf_size
    }

    /// Return the maximum number of concurrent accepted connections.
    pub fn max_connections(&self) -> usize {
        self.max_connections
    }

    /// Return the maximum accepted request body size.
    pub fn max_request_size(&self) -> usize {
        self.max_request_size
    }

    /// Return the graceful shutdown timeout, capped at 30 seconds.
    pub fn shutdown_timeout(&self) -> Duration {
        self.shutdown_timeout.min(Duration::from_secs(30))
    }

    /// Return the HTTP/1 header read timeout, capped at 30 seconds.
    pub fn http1_header_read_timeout(&self) -> Duration {
        self.http1_header_read_timeout.min(Duration::from_secs(30))
    }
}

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
impl ServerConfig {
    /// Return the configured HTTP/2 max concurrent streams value.
    pub fn http2_max_concurrent_streams(&self) -> Option<u32> {
        self.http2_max_concurrent_streams
    }

    /// Return the configured HTTP/2 maximum send buffer size.
    pub fn http2_max_send_buf_size(&self) -> usize {
        self.http2_max_send_buf_size
    }

    /// Return the configured TLS handshake timeout.
    pub fn tls_handshake_timeout(&self) -> Duration {
        self.tls_handshake_timeout
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            keep_alive: true,
            max_buf_size: 16384, // 16 KB
            max_connections: DEFAULT_MAX_CONNECTIONS - RUNTIME_FD_BUDGET,
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
