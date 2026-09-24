use hyper::server::conn::*;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use std::io;
use std::process::ExitCode;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::time::timeout;

use super::cancel::CancellationToken;
use super::io::IoWithPermit;
use super::js::JoinSet;
use super::tls::{Acceptor, Alpn};
use crate::app::ServiceAdapter;
use crate::server::tls::NegotiateAlpn;

pub(super) async fn accept<App, Protocol>(
    service: ServiceAdapter<App>,
    protocol: Protocol,
    listener: TcpListener,
) -> ExitCode
where
    App: Send + Sync + 'static,
    Protocol: Acceptor,
    Protocol::Stream: Send + Unpin + 'static,
{
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
    let (recycler, mut connections) = JoinSet::new(service.config().max_num_cohorts());

    // Notify connection tasks when a shutdown signal is received by the process.
    let cancellation = CancellationToken::new();

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
        let semaphore = semaphore.clone();

        // Either accept the next connection from the TCP listener or receive a
        // shutdown signal.
        tokio::select! {
            // TCP stream accepted.
            result = listener.accept() => match result {
                Ok(accepted) => {
                    // Acquire a permit and proceed with serving the connection.
                    //
                    // The maximum number of permits is 1 away from EMFILE on
                    // linux so we acquire the permit afterwards to determine
                    // if we should shed the load to mitigate the risk of an
                    // EMFILE entirely.
                    //
                    // We could instead, await the semaphore permit at the start
                    // of the loop but that would result in more connections
                    // being queued by the OS.
                    if let Ok(permit) = semaphore.try_acquire_owned() {
                        let handshake = protocol.accept(accepted.0, permit);
                        let new_service = service.clone();
                        let cancellation = cancellation.clone();

                        connections.spawn(handle_conn(handshake, new_service, cancellation));

                        if connections.size() >= service.config().cohort_size() {
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
            did_panic = cancellation.wait() => {
                if did_panic {
                    return ExitCode::FAILURE;
                } else {
                    let duration = service.config().shutdown_timeout();

                    if timeout(duration, connections.join_all()).await.is_ok() {
                        return ExitCode::SUCCESS;
                    } else {
                        return ExitCode::FAILURE;
                    }
                }
            }
        };
    }
}

async fn handle_conn<App, Io, F>(
    handshake: F,
    service: ServiceAdapter<App>,
    waiter: CancellationToken,
) where
    App: Send + Sync + 'static,
    Io: AsyncRead + AsyncWrite + NegotiateAlpn + Send + Unpin + 'static,
    F: Future<Output = io::Result<IoWithPermit<Io>>> + Send + 'static,
{
    match handshake.await {
        Ok(stream) => {
            if stream.preferred_alpn() == Alpn::HTTP_2 {
                waiter.observe(http_2_conn(stream, service)).await;
            } else {
                waiter.observe(http_11_conn(stream, service)).await;
            }
        }
        Err(error) => {
            log!(error(tls = 0), "{}", &error);
        }
    }
}

fn http_11_conn<Io, App>(
    stream: IoWithPermit<Io>,
    service: ServiceAdapter<App>,
) -> http1::UpgradeableConnection<IoWithPermit<Io>, ServiceAdapter<App>>
where
    Io: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    App: Send + Sync + 'static,
{
    http1::Builder::new()
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
        .with_upgrades()
}

fn http_2_conn<Io, App>(
    stream: IoWithPermit<Io>,
    service: ServiceAdapter<App>,
) -> http2::Connection<IoWithPermit<Io>, ServiceAdapter<App>, TokioExecutor>
where
    Io: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    App: Send + Sync + 'static,
{
    http2::Builder::new(TokioExecutor::new())
        .adaptive_window(false)
        .auto_date_header(true)
        .max_header_list_size(16384) // 16 KB
        .initial_connection_window_size(Some(1048576)) // 1 MB
        .initial_stream_window_size(Some(65536)) // 64 MB
        .max_frame_size(Some(16384)) // 16 KB
        .max_concurrent_streams(service.config().http2_max_concurrent_streams())
        .max_send_buf_size(service.config().http2_max_send_buf_size())
        .timer(TokioTimer::new())
        .serve_connection(stream, service)
}
