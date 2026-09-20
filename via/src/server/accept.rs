use hyper::server::conn::*;
use hyper_util::rt::TokioTimer;
use std::process::ExitCode;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::time::timeout;

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
use hyper_util::rt::TokioExecutor;

use super::cancel::Cancellation;
use super::io::IoWithPermit;
use super::join_set::{self, JoinSet};
use super::tls::Acceptor;
use crate::app::ServiceAdapter;

#[cfg(not(any(feature = "native-tls", feature = "rustls-23")))]
use super::tcp::TcpStream;

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
use super::tls::Alpn;

macro_rules! serve_unless_cancelled {
    ($connection:ident, $cancellation:ident) => {{
        let result = tokio::select! {
            // The connection future is ready.
            result = &mut $connection => result,
            // A graceful shutdown signal was sent to the process.
            _ = $cancellation.wait() => {
                let mut $connection = Pin::new(&mut $connection);
                $connection.as_mut().graceful_shutdown();
                $connection.await
            }
        };

        if let Err(ref error) = result {
            #[cfg(not(debug_assertions))]
            let _ = error; // Placeholder for tracing...
            log!(info(service = 0), "{}", &error);
        }
    }};
}

pub(super) async fn accept<App, Tls>(
    acceptor: Tls,
    listener: TcpListener,
    service: ServiceAdapter<App>,
) -> ExitCode
where
    App: Send + Sync + 'static,
    Tls: Acceptor + 'static,
    Tls::Error: Send + 'static,
    Tls::Stream: Send + Unpin + 'static,
{
    #[cfg(not(any(feature = "native-tls", feature = "rustls-23")))]
    let _ = acceptor;

    // Connection bookkeeping occurs in `JoinSet`. Connections that survive
    // more than a single cohort generation "detach" (i.e websockets).
    //
    // The `JoinSet` layout is recycled when possible. However, an entire cohort
    // can detach if the server is experiencing an abnormally high-volume of
    // concurrent connections.
    //
    // This allows the `JoinSet` to quarantine retired cohorts when necessary and
    // temporally decouples an allocation from load.
    //
    // Users of Via that wish to retain the same pair of join set cohorts for the
    // entire runtime of their program can determine the amount of time required
    // to join enough connections to accommodate their users without detaching an
    // entire cohort and use the metric to configure rate-limiting in the network
    // tier (API gateway, reverse-proxy, load balancer, etc.).
    let (recycler, mut connections) = JoinSet::new();

    // Notify connection tasks when a shutdown signal is received by the process.
    let cancellation = Cancellation::new();

    // Provides a "soft" upper-bound on concurrency.
    //
    // When there no more permits available, the connection is reset. For this
    // reason, we suggest having at least one other node in your Via cluster.
    //
    // Various HTTP-aware load balancers support retrying non-idempotent requests
    // on RST. If configured properly, the resulting infrastructure intersects
    // assurance with availability that resembles telcom.
    let semaphore = {
        let max_connections = service.config().max_connections();

        if max_connections <= 1 {
            log!(error(accept = 0), "max_connections must be > 10");
            return ExitCode::FAILURE;
        }

        // Keep one connection slot available for `accept()` itself.
        // When all permitted connection slots are occupied, RST.
        Arc::new(Semaphore::new(max_connections - 1))
    };

    // Start accepting incoming connections.
    loop {
        // Eagerly clone the semaphore. It is low-contention and used frequently.
        let semaphore = semaphore.clone();

        // Either accept the next connection from the TCP listener or receive a
        // shutdown signal.
        //
        // The listener is always polled before the shutdown signal.
        tokio::select! {
            biased; // Poll `listener.accept()` before `cancellation.wait()`.

            // TCP stream accepted.
            result = listener.accept() => match result {
                Ok((stream, _)) => {
                    // Acquire a permit and proceed with serving the connection.
                    //
                    // The maximum number of permits is 1 away from EMFILE on
                    // linux so we acquire the permit afterwards to determine
                    // if we should shed the load to mitigate the risk of an
                    // EMFILE entirely.
                    //
                    // We could instead, await the semaphore permit at the start
                    // of the loop but that would result in more connections
                    // being queued by the os.
                    if let Ok(permit) = semaphore.try_acquire_owned() {
                        #[cfg(any(feature = "native-tls", feature = "rustls-23"))]
                        let future = acceptor.accept(permit, stream);

                        let service = service.clone();
                        let cancellation = cancellation.clone();

                        #[cfg(any(feature = "native-tls", feature = "rustls-23"))]
                        connections.spawn(serve_tls::<_, Tls>(future, service, cancellation));

                        #[cfg(not(any(feature = "native-tls", feature = "rustls-23")))]
                        connections.spawn(serve_tcp(stream, permit, service, cancellation));

                        if connections.size() >= join_set::COHORT_SIZE {
                            let recycler = recycler.clone();
                            connections.rotate(recycler);
                        }
                    }
                }
                Err(error) => {
                    // Print the error message to stderr in debug builds.
                    log!(error(accept = 0), "{}", error);

                    return cfg_select! {
                        unix => match error.raw_os_error() {
                            // ENOMEM or ENFILE
                            //
                            // Immutably replacing the node is preferred when
                            // the process exits with any of these codes.
                            Some(code @ (12 | 23)) => ExitCode::from(code as u8),

                            // EMFILE
                            //
                            // This should never happen.
                            Some(24) => ExitCode::from(24),

                            // All other codes are an opaque error.
                            //
                            // Follow the instructions provided for non-POSIX
                            // systems.
                            _ => ExitCode::FAILURE,
                        },

                        // Use an opaque exit code for non-POSIX platforms.
                        //
                        // Either restart the process or immutably replace
                        // the node.
                        //
                        // When possible, prefer containerized immutable
                        // deployments.
                        _ => ExitCode::FAILURE,
                    };
                }
            },

            // Shutdown request received.
            _ = cancellation.wait() => {
                let graceful_shutdown = timeout(
                    service.config().shutdown_timeout(),
                    connections.join_all(),
                );

                return if graceful_shutdown.await.is_ok() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                };
            }
        };
    }
}

async fn serve_http_11<App, Io>(
    stream: IoWithPermit<Io>,
    service: ServiceAdapter<App>,
    cancellation: Cancellation,
) where
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
        .serve_connection(stream, service)
        .with_upgrades();

    serve_unless_cancelled!(connection, cancellation);
}

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
async fn serve_http_2<App, Io>(
    stream: IoWithPermit<Io>,
    service: ServiceAdapter<App>,
    cancellation: Cancellation,
) where
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
        .serve_connection(stream, service);

    serve_unless_cancelled!(connection, cancellation);
}

#[cfg(any(feature = "native-tls", feature = "rustls-23"))]
async fn serve_tls<App, Tls>(
    future: impl Future<Output = Result<IoWithPermit<Tls::Stream>, Tls::Error>> + Send + 'static,
    service: ServiceAdapter<App>,
    cancellation: Cancellation,
) where
    App: Send + Sync + 'static,
    Tls: Acceptor,
    Tls::Error: Send + 'static,
    Tls::Stream: Send + Unpin + 'static,
{
    match timeout(service.config().tls_handshake_timeout(), future).await {
        Ok(Ok(stream)) => {
            if stream.preferred_alpn() == Alpn::HTTP_2 {
                serve_http_2(stream, service, cancellation).await;
            } else {
                serve_http_11(stream, service, cancellation).await;
            }
        }
        Ok(Err(error)) => {
            log!(error(tls = 0), "{}", &error);
        }
        Err(_timeout) => {
            log!(
                error(tls = 0),
                "handshake did not complete within tls_handshake_timeout"
            );
        }
    }
}

#[cfg(not(any(feature = "native-tls", feature = "rustls-23")))]
async fn serve_tcp<App>(
    stream: tokio::net::TcpStream,
    permit: tokio::sync::OwnedSemaphorePermit,
    service: ServiceAdapter<App>,
    cancellation: Cancellation,
) where
    App: Send + Sync + 'static,
{
    let stream = IoWithPermit::new(TcpStream::new(stream), permit);
    serve_http_11(stream, service, cancellation).await;
}
