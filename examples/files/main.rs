use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use tokio::sync::Semaphore;
use via::{Error, Next, Request, Server};

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

#[cfg_attr(not(feature = "file"), allow(dead_code))]
struct Unicorn {
    /// The directory from which files can be served.
    public_dir: Box<Path>,

    /// Limit concurrent requests to prevent exit on EMFILE (for safety).
    /// Via prefers that fd back-pressure be a user-space responsibility.
    semaphore: Arc<Semaphore>,
}

#[cfg_attr(not(feature = "file"), allow(dead_code))]
impl Unicorn {
    fn public_dir(&self) -> &Path {
        &self.public_dir
    }

    fn semaphore(&self) -> &Arc<Semaphore> {
        &self.semaphore
    }
}

/// Calculates how many files can be open concurrently and returns it along
/// with the adjusted maximum number of connections.
fn determine_resource_usage() -> via::Result<(usize, usize)> {
    cfg_select! {
        // On Windows, sockets are not files.
        windows => Ok((MAX_CONNECTIONS.div_ceil(2), MAX_CONNECTIONS)),

        // On POSIX, max_concurrency affects the max_connections budget.
        //
        // The default amount of padding provided by Via is 13. The maximum
        // amount of concurrent file streams should not exceed the number of
        // worker process in the tokio runtime.
        _ => {{
            let concurrency = std::thread::available_parallelism()?.get();
            let adjusted_max = MAX_CONNECTIONS - RT_FD_REQUIREMENT - concurrency;

            Ok((concurrency, adjusted_max))
        }}
    }
}

/// Extracts the relative path to the requested file from the request.
#[cfg(feature = "file")]
fn extract_file_path(request: &Request<Unicorn>) -> via::Result<PathBuf> {
    let public_dir = request.app().public_dir();

    if let Some(path_param) = request.param("path").ok()?.as_deref() {
        Ok(public_dir.join(path_param))
    } else {
        Ok(public_dir.join("index.html"))
    }
}

/// Resolve a relative path to the public directory.
fn resolve_public_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    if manifest_dir.ends_with("examples") {
        manifest_dir.join("files/public")
    } else {
        manifest_dir.join("examples/files/public")
    }
}

#[cfg(feature = "file")]
async fn serve_dir(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
    use std::ffi::OsStr;
    use via::response::File;

    let permit = request.app().semaphore().clone().acquire_owned().await?;
    let mut path = extract_file_path(&request)?;
    let content_type = if let Some(extension) = path.extension().and_then(OsStr::to_str) {
        mime_guess::from_ext(extension).first_or_octet_stream()
    } else {
        path.set_extension("html");
        mime_guess::mime::TEXT_HTML_UTF_8
    };

    File::open(path)
        .content_type(&content_type)
        .with_last_modified()
        .permit(permit)
        .serve(1024 * 1024) // stream files > 1 MB
        .await
}

#[cfg(not(feature = "file"))]
async fn serve_dir(_: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
    panic!("");
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let (max_open_files, max_connections) = determine_resource_usage()?;
    let mut app = via::app(Unicorn {
        public_dir: resolve_public_dir().into_boxed_path(),
        semaphore: Arc::new(Semaphore::new(max_open_files)),
    });

    app.route("/*path").to(via::get(serve_dir));

    if cfg!(not(feature = "file")) {
        eprintln!("    the \"file\" feature flag is required in order to run the files example.");
        eprintln!("    re-run this example with cargo run --example files --feature=\"file\"");
        return Ok(ExitCode::FAILURE);
    }

    Server::new(app)
        .max_connections(max_connections)
        .listen(("127.0.0.1", 8080))
        .await
}
