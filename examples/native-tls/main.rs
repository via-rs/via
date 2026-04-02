use native_tls::Identity;
use std::path::{Path, PathBuf};
use std::{fs, process::ExitCode};
use via::{Error, Next, Request, Response, ResultExt, Server};

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");
const TLS_PKCS_PASSWORD: &str = "TLS_PKCS_PASSWORD";

async fn hello(request: Request, _: Next) -> via::Result {
    // Get a reference to the path parameter `name` from the request uri.
    let name = request.param("name").percent_decode().into_result()?;

    // Send a plain text response with our greeting message.
    Response::build().text(format!("Hello, {}! (via TLS)", name))
}

fn load_pkcs12(from: &Path) -> Result<Identity, Error> {
    let p12_path = from.join("localhost.p12");
    let identity = fs::read(&p12_path).unwrap_or_else(|_| {
        panic!("failed to load pkcs#12 file from: {:?}", p12_path);
    });

    let password = std::env::var(TLS_PKCS_PASSWORD).unwrap_or_else(|_| {
        panic!("missing required env var \"{}\"", TLS_PKCS_PASSWORD);
    });

    Ok(Identity::from_pkcs12(&identity, &password)?)
}

fn resolve_example_dir() -> PathBuf {
    let manifest_dir = Path::new(CARGO_MANIFEST_DIR);

    if CARGO_MANIFEST_DIR.ends_with("examples") {
        manifest_dir.join("native-tls")
    } else {
        manifest_dir.join("examples/native-tls")
    }
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let example_dir = resolve_example_dir();

    // Load our .env file containing TLS_PKCS_PASSWORD.
    dotenvy::from_filename(example_dir.join(".env"))?;

    // Make sure that our TLS config is present and valid before we proceed.
    let tls_config = load_pkcs12(&example_dir)?;

    let mut app = via::app(());

    // Add our hello responder to the endpoint /hello/:name.
    app.route("/hello/:name").to(via::get(hello));

    Server::new(app)
        .listen_native_tls(("127.0.0.1", 8080), tls_config)
        .await
}
