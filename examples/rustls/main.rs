use std::path::{Path, PathBuf};
use std::process::ExitCode;
use via::{Error, Next, Request, Response, Server};

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

async fn hello(request: Request, _: Next) -> via::Result {
    // Get a reference to the path parameter `name` from the request uri.
    let name = request.param("name").percent_decode().ok_or_bad_request()?;

    // Send a plain text response with our greeting message.
    Response::build().text(format!("Hello, {}! (via TLS)", name))
}

fn resolve_example_dir() -> PathBuf {
    let manifest_dir = Path::new(CARGO_MANIFEST_DIR);

    if CARGO_MANIFEST_DIR.ends_with("examples") {
        manifest_dir.join("rustls")
    } else {
        manifest_dir.join("examples/rustls")
    }
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let example_dir = resolve_example_dir();

    // Make sure that our TLS config is present and valid before we proceed.
    let tls_config = tls::server_config(&example_dir)?;

    let mut app = via::app(());

    // Add our hello responder to the endpoint /hello/:name.
    app.route("/hello/:name").to(via::get(hello));

    Server::new(app)
        .listen_rustls_23(("127.0.0.1", 8080), tls_config)
        .await
}

mod tls {
    // This module is inspired by the server example in the tokio-rustls repo:
    // https://github.com/rustls/tokio-rustls/blob/main/examples/server.rs
    //

    use std::fs;
    use std::path::Path;
    use tokio_rustls::rustls;
    use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use via::Error;

    /// Load the certificate and private key from the file system and use them
    /// to create a rustls::ServerConfig.
    ///
    pub fn server_config(load_from: &Path) -> Result<rustls::ServerConfig, Error> {
        let key = PrivateKeyDer::Pkcs1(fs::read(load_from.join("localhost.key"))?.into());
        let cert = CertificateDer::from(fs::read(load_from.join("localhost.cert"))?);
        let mut config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .expect("tls config is invalid or missing");

        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        Ok(config)
    }
}
