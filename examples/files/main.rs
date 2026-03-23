use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use tokio::sync::Semaphore;
use via::{Error, Middleware, Next, Request, Server};

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");
const MAX_CONNECTIONS: usize = 500;
const MAX_ALLOC_SIZE: usize = 1024 * 1024; // 1 MB

macro_rules! trym {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => return Box::pin(async { Err(error) }),
        }
    };
}

struct ServeFrom {
    /// The directory from which files can be served.
    public_dir: PathBuf,

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
    let manifest_dir = Path::new(CARGO_MANIFEST_DIR);

    if CARGO_MANIFEST_DIR.ends_with("examples") {
        manifest_dir.join("files/public")
    } else {
        manifest_dir.join("examples/files/public")
    }
}

impl ServeFrom {
    fn new(concurrency: usize, public_dir: impl AsRef<Path>) -> Self {
        Self {
            public_dir: public_dir.as_ref().to_path_buf(),
            semaphore: Arc::new(Semaphore::new(concurrency)),
        }
    }
}

#[cfg(not(feature = "fs"))]
impl Middleware<()> for ServeFrom {
    fn call(&self, _: Request, _: Next) -> via::BoxFuture {
        unreachable!()
    }
}

#[cfg(feature = "fs")]
impl Middleware<()> for ServeFrom {
    fn call(&self, request: Request, _: Next) -> via::BoxFuture {
        use std::ffi::OsStr;
        use via::response::File;

        let path = trym!(request.param("path").ok()).unwrap_or("index.html".into());
        let mut path = self.public_dir.join(path.as_ref());
        let semaphore = self.semaphore.clone();

        Box::pin(async move {
            let permit = semaphore.acquire_owned().await?;

            let mime_type = if let Some(ext) = path.extension().and_then(OsStr::to_str) {
                mime_guess::from_ext(ext).first_or_octet_stream()
            } else {
                path.set_extension("html");
                mime_guess::mime::TEXT_HTML_UTF_8
            };

            let response = File::open(&path)
                .content_type(&mime_type)
                .with_last_modified()
                .serve(MAX_ALLOC_SIZE) // stream files > 1 MB
                .await?;

            // TODO: add an optional permit field or close callback to file.
            drop(permit);

            Ok(response)
        })
    }
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let mut app = via::app(());
    let public_dir = resolve_public_dir();
    let file_server = ServeFrom::new(MAX_CONNECTIONS, &public_dir);

    if cfg!(not(feature = "fs")) {
        panic!("the \"fs\" feature must be enabled in order to serve files.");
    }

    println!("serving files from: {:?}", &public_dir);

    app.route("/*path").to(via::get(file_server));

    Server::new(app)
        .max_connections(MAX_CONNECTIONS * 2)
        .listen(("127.0.0.1", 8080))
        .await
}
