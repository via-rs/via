use hyper::server::conn::*;
use hyper_util::rt::TokioTimer;
use std::mem;
use std::process::ExitCode;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::{JoinSet, coop};
use tokio::time::timeout;

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
use hyper_util::rt::TokioExecutor;

use super::cancel::Cancellation;
use super::io::IoWithPermit;
use super::tls::Acceptor;
use crate::app::ServiceAdapter;
use crate::error::ServerError;

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
use super::tls::{Alpn, NegotiateAlpn};

macro_rules! log {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            eprintln!($($arg)*)
        }
    };
}

macro_rules! serve_unless_cancelled {
    ($cancellation:ident, $connection:ident) => {
        tokio::select! {
            // The connection future is ready.
            result = &mut $connection => result?,
            // A graceful shutdown signal was sent to the process.
            _ = $cancellation.wait() => {
                let mut $connection = Pin::new(&mut $connection);
                $connection.as_mut().graceful_shutdown();
                $connection.await?;
            }
        }
    };
}

pub(super) async fn accept<App, TlsAcceptor>(
    acceptor: TlsAcceptor,
    listener: TcpListener,
    service: ServiceAdapter<App>,
) -> ExitCode
where
    App: Send + Sync + 'static,
    ServerError: From<TlsAcceptor::Error>,
    TlsAcceptor: Acceptor,
    TlsAcceptor::Stream: Send + Unpin + 'static,
{
    #[cfg(not(any(feature = "native-tls", feature = "rustls-23")))]
    drop(acceptor);

    // Create a semaphore with a number of permits equal to the maximum number
    // of connections that the server can handle concurrently.
    let semaphore = Arc::new(Semaphore::new(service.config().max_connections()));

    // A JoinSet to track and join active connections.
    let mut connections = JoinSet::new();

    // Notify the accept loop and connection tasks to initiate a graceful
    // shutdown when a "ctrl-c" notification is sent to the process.
    let cancellation = wait_for_ctrl_c();

    // Start accepting incoming connections.
    let exit_code = loop {
        let (io, _) = tokio::select! {
            // A new TCP stream was accepted from the listener.
            result = listener.accept() => match result {
                Ok(stream) => stream,
                Err(error) => {
                    log!("error(accept): {}", error);

                    #[cfg(unix)]
                    let Some(12 | 23 | 24) = error.raw_os_error() else {
                        continue;
                    };

                    #[cfg(windows)]
                    let Some(10024 | 10055) = error.raw_os_error() else {
                        continue;
                    };

                    break ExitCode::FAILURE;
                }
            },
            // A graceful shutdown signal was sent to the process.
            _ = cancellation.wait() => {
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
        let cancellation = cancellation.clone();

        // Spawn a task to serve the connection.
        #[cfg(any(feature = "native-tls", feature = "rustls-23"))]
        connections.spawn({
            let handshake = acceptor.accept(io);

            // native-tls task size: 1952
            // rustls task size: 1752
            async move {
                let io = timeout(service.config().tls_handshake_timeout(), handshake).await??;

                if *io.preferred_alpn() == Alpn::HTTP_2 {
                    let io = IoWithPermit::new(io, permit);
                    let serve = serve_http2_connection(io, service, cancellation);

                    serve.await
                } else {
                    let io = IoWithPermit::new(io, permit);
                    let serve = serve_http1_connection(io, service, cancellation);

                    serve.await
                }
            }
        });

        // task size: 928
        #[cfg(not(any(feature = "native-tls", feature = "rustls-23")))]
        connections.spawn(async move {
            let io = IoWithPermit::new(io, permit);
            let serve = serve_http1_connection(io, service, cancellation);

            serve.await
        });

        if connections.len() >= 1024 {
            let batch = mem::take(&mut connections);
            tokio::spawn(drain_connections(false, batch));
        }
    };

    // Try to drain each inflight connection before `config.shutdown_timeout`.
    match timeout(
        service.config().shutdown_timeout(),
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

async fn serve_http1_connection<App, Io>(
    io: IoWithPermit<Io>,
    service: ServiceAdapter<App>,
    cancellation: Cancellation,
) -> Result<(), ServerError>
where
    App: Send + Sync + 'static,
    Io: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let mut connection = http1::Builder::new()
        .allow_multiple_spaces_in_request_line_delimiters(false)
        .auto_date_header(true)
        .half_close(false)
        .ignore_invalid_headers(false)
        .keep_alive(service.config().keep_alive())
        .max_buf_size(service.config().max_buf_size())
        .pipeline_flush(false)
        .preserve_header_case(false)
        .header_read_timeout(Some(service.config().http1_header_read_timeout()))
        .timer(TokioTimer::new())
        .title_case_headers(false)
        .serve_connection(io, service)
        .with_upgrades();

    serve_unless_cancelled!(cancellation, connection);
    Ok(())
}

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
async fn serve_http2_connection<App, Io>(
    io: IoWithPermit<Io>,
    service: ServiceAdapter<App>,
    cancellation: Cancellation,
) -> Result<(), ServerError>
where
    App: Send + Sync + 'static,
    Io: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let mut connection = http2::Builder::new(TokioExecutor::new())
        .adaptive_window(false)
        .auto_date_header(true)
        .max_header_list_size(16384) // 16 KB
        .initial_connection_window_size(Some(1048576)) // 1 MB
        .initial_stream_window_size(Some(65536)) // 64 MB
        .max_frame_size(Some(16384)) // 16 KB
        .max_concurrent_streams(service.config().http2_max_concurrent_streams())
        .max_send_buf_size(service.config().http2_max_send_buf_size())
        .timer(TokioTimer::new())
        .serve_connection(io, service);

    serve_unless_cancelled!(cancellation, connection);

    Ok(())
}

fn wait_for_ctrl_c() -> Cancellation {
    let (cancellation, remote) = Cancellation::new();

    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_err() {
            eprintln!("unable to register the 'ctrl-c' signal.");
        }

        remote.cancel();
    });

    cancellation
}
