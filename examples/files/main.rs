use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::{env, thread};
use tokio::sync::Semaphore;
use via::{Error, Server};

#[cfg(feature = "fs")]
use via::{Next, Request, ResultExt};

/// The maximum number of connections we are willing to accept before
/// accounting for resources other than TCP connections.
const MAX_CONNECTIONS: usize = 1024;

/// The number of file descriptors required for the multi-threaded tokio
/// runtime, a graceful shutdown signal, and a tcp listener.
///
/// This is used to provide padding for the runtime so we can deterministically
/// calculate the number of file descriptors required for the server to never
/// trigger an EMFILE.
const RT_FD_REQUIREMENT: usize = 13;

struct Unicorn {
    /// The directory from which files can be served.
    public_dir: Arc<Path>,

    /// Limit concurrent requests to prevent exit on EMFILE (for safety).
    /// Via prefers that fd back-pressure be a user-space responsibility.
    semaphore: Arc<Semaphore>,
}

/// Resolve a relative path to the public directory.
///
/// This example can be run from the workspace root or the examples directory.
/// Therefore, we must determine the context in which this example was
/// compiled. Then,
///
fn resolve_public_dir() -> PathBuf {
    const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

    let manifest_dir = Path::new(CARGO_MANIFEST_DIR);

    if CARGO_MANIFEST_DIR.ends_with("examples") {
        manifest_dir.join("files/public")
    } else {
        manifest_dir.join("examples/files/public")
    }
}

#[cfg(feature = "fs")]
async fn serve_dir(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
    use std::ffi::OsStr;
    use via::response::File;

    let semaphore = request.app().semaphore.clone();
    let permit = semaphore.acquire_owned().await?;

    let mut path = {
        let path_param = request
            .param("path")
            .into_result()
            .unwrap_or("index.html".into());

        request.app().public_dir.join(path_param.as_ref())
    };

    let mime_type = if let Some(ext) = path.extension().and_then(OsStr::to_str) {
        mime_guess::from_ext(ext).first_or_octet_stream()
    } else {
        path.set_extension("html");
        mime_guess::mime::TEXT_HTML_UTF_8
    };

    File::open(path)
        .content_type(&mime_type)
        .with_last_modified()
        .permit(permit)
        .serve(1024 * 1024) // stream files > 1 MB
        .await
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let (max_concurrency, max_connections) = cfg_select! {
        // On Windows, sockets are not files.
        windows => (MAX_CONNECTIONS.div_ceil(2), MAX_CONNECTIONS),

        // On POSIX, max_concurrency affects the max_connections budget.
        //
        // The default amount of padding provided by Via is 13. The maximum
        // amount of concurrent file streams should not exceed the number of
        // worker process in the tokio runtime.
        _ => {{
            let concurrency = thread::available_parallelism()?.get();
            let adjusted_max = MAX_CONNECTIONS - RT_FD_REQUIREMENT - concurrency;

            (concurrency, adjusted_max)
        }}
    };

    let public_dir = resolve_public_dir();

    if cfg!(not(feature = "fs")) {
        panic!("the \"fs\" feature must be enabled in order to serve files.");
    }

    if cfg!(debug_assertions) {
        println!("serving files from: {:?}", &public_dir);
        println!(
            "  max concurrency = {}, max connections = {}",
            max_concurrency, max_connections
        );
    }

    let mut app = via::app(Unicorn {
        public_dir: public_dir.into(),
        semaphore: Arc::new(Semaphore::new(max_concurrency)),
    });

    app.route("/*path").to(via::get(serve_dir));

    Server::new(app)
        .max_connections(max_connections)
        .listen(("127.0.0.1", 8080))
        .await
}
