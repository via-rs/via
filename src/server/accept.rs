use hyper::server::conn;
use hyper_util::rt::TokioTimer;
use std::mem;
use std::process::ExitCode;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::{JoinSet, coop};
use tokio::{signal, time};
use tokio_util::sync::{CancellationToken, WaitForCancellationFuture};

use super::{IoWithPermit, ServerConfig, tls};
use crate::app::AppService;
use crate::error::ServerError;

#[derive(Clone)]
struct InitializationToken(CancellationToken);

macro_rules! log {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            eprintln!($($arg)*)
        }
    };
}

pub(super) async fn accept<App, TlsAcceptor>(
    acceptor: TlsAcceptor,
    listener: TcpListener,
    service: AppService<App>,
    config: ServerConfig,
) -> ExitCode
where
    App: Send + Sync + 'static,
    ServerError: From<TlsAcceptor::Error>,
    TlsAcceptor: tls::Acceptor,
    TlsAcceptor::Io: Send + Unpin + 'static,
{
    #[cfg(not(any(feature = "native-tls", feature = "rustls")))]
    drop(acceptor);

    // Create a semaphore with a number of permits equal to the maximum number
    // of connections that the server can handle concurrently.
    let semaphore = Arc::new(Semaphore::new(config.max_connections));

    // A JoinSet to track and join active connections.
    let mut connections = JoinSet::new();

    // Notify the accept loop and connection tasks to initiate a graceful
    // shutdown when a "ctrl-c" notification is sent to the process.
    let shutdown = wait_for_ctrl_c();

    // Start accepting incoming connections.
    let exit_code = loop {
        let (io, _) = tokio::select! {
            // Keep tail latency low when the server is at capacity.
            biased;

            // A new TCP stream was accepted from the listener.
            result = listener.accept() => match result {
                Ok(stream) => stream,
                Err(error) => return match error.raw_os_error() {
                    Some(10024 | 10055) if cfg!(windows) => ExitCode::FAILURE,
                    Some(12 | 23 | 24) if cfg!(unix) => ExitCode::FAILURE,
                    _ => {
                        log!("error(accept): {}", error);
                        continue;
                    }
                },
            },

            // A graceful shutdown signal was sent to the process.
            _ = shutdown.requested() => {
                break ExitCode::SUCCESS;
            }
        };

        // Permit acquired. Proceed with serving the connection.
        let Ok(permit) = semaphore.clone().try_acquire_owned() else {
            // The server is at capacity. Close the connection. Upstream load
            // balancers take this as a hint that it is time to try another
            // node.
            continue;
        };

        let service = service.clone();
        let shutdown = shutdown.clone();

        // Spawn a task to serve the connection.
        #[cfg(any(feature = "native-tls", feature = "rustls"))]
        connections.spawn({
            let timeout = config.tls_handshake_timeout;
            let handshake = acceptor.accept(io);

            async move {
                let (io, alpn) = time::timeout(timeout, handshake).await??;
                let io = IoWithPermit::new(io, permit);

                if alpn == tls::Alpn::H2 {
                    serve_h2_connection(io, service, shutdown).await
                } else {
                    serve_connection(io, service, shutdown).await
                }
            }
        });

        #[cfg(not(any(feature = "native-tls", feature = "rustls")))]
        connections.spawn(async move {
            let io = IoWithPermit::new(io, permit);
            serve_connection(io, service, shutdown).await
        });

        if connections.len() >= 1024 {
            let batch = mem::take(&mut connections);
            tokio::spawn(drain_connections(false, batch));
        }
    };

    // Try to drain each inflight connection before `config.shutdown_timeout`.
    match time::timeout(
        config.shutdown_timeout,
        drain_connections(true, connections),
    )
    .await
    {
        Ok(_) => exit_code,
        Err(_) => ExitCode::FAILURE,
    }
}

async fn drain_connections(immediate: bool, mut connections: JoinSet<Result<(), ServerError>>) {
    log!("joining {} inflight connections...", connections.len());

    while let Some(result) = connections.join_next().await {
        match result {
            Ok(Ok(_)) => {}
            Err(error) => log!("error(connection): {}", error),
            Ok(Err(error)) => log!("error(service): {}", error),
        }

        if !immediate {
            coop::consume_budget().await;
        }
    }
}

async fn serve_connection<App, Io>(
    io: IoWithPermit<Io>,
    service: AppService<App>,
    shutdown: InitializationToken,
) -> Result<(), ServerError>
where
    App: Send + Sync + 'static,
    Io: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let connection = conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .serve_connection(io, service)
        .with_upgrades();

    tokio::pin!(connection);
    tokio::select! {
        // Keep tail latency low when the server is at capacity.
        biased;

        // The connection future is ready.
        result = &mut connection => Ok(result?),

        // A graceful shutdown signal was sent to the process.
        _ = shutdown.requested() => {
            connection.as_mut().graceful_shutdown();
            Ok((&mut connection).await?)
        }
    }
}

#[cfg(any(feature = "native-tls", feature = "rustls"))]
async fn serve_h2_connection<App, Io>(
    io: IoWithPermit<Io>,
    service: AppService<App>,
    shutdown: InitializationToken,
) -> Result<(), ServerError>
where
    App: Send + Sync + 'static,
    Io: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let connection = conn::http2::Builder::new(hyper_util::rt::TokioExecutor::new())
        .timer(TokioTimer::new())
        .serve_connection(io, service);

    tokio::pin!(connection);
    tokio::select! {
        // Keep tail latency low when the server is at capacity.
        biased;

        // The connection future is ready.
        result = &mut connection => Ok(result?),

        // A graceful shutdown signal was sent to the process.
        _ = shutdown.requested() => {
            connection.as_mut().graceful_shutdown();
            Ok((&mut connection).await?)
        }
    }
}

fn wait_for_ctrl_c() -> InitializationToken {
    let token = InitializationToken::new();
    let shutdown = token.clone();

    tokio::spawn(async move {
        if signal::ctrl_c().await.is_err() {
            eprintln!("unable to register the 'ctrl-c' signal.");
        }

        shutdown.start();
    });

    token
}

impl InitializationToken {
    fn new() -> Self {
        Self(CancellationToken::new())
    }

    fn requested(&self) -> WaitForCancellationFuture<'_> {
        self.0.cancelled()
    }

    fn start(&self) {
        self.0.cancel();
    }
}
