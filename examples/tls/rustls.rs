use std::path::PathBuf;
use std::process::ExitCode;
use via::{Error, Next, Request, Response, Server};

async fn hello(request: Request, _: Next) -> via::Result {
    // Get a reference to the path parameter `name` from the request uri.
    let name = request.param("name").decode().into_result()?;

    // Send a plain text response with our greeting message.
    Response::build().text(format!("Hello, {}! (via TLS)", name))
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    // Make sure that our TLS config is present and valid before we proceed.
    let tls_config = {
        let path_to_this_file = PathBuf::from(file!());
        tls::server_config(path_to_this_file.parent().unwrap())
    };

    let mut app = via::app(());

    // Add our hello responder to the endpoint /hello/:name.
    app.route("/hello/:name").to(via::get(hello));

    Server::new(app)
        .listen_rustls(("127.0.0.1", 8080), tls_config)
        .await
}

mod tls {
    // This module is inspired by the server example in the tokio-rustls repo:
    // https://github.com/rustls/tokio-rustls/blob/main/examples/server.rs
    //

    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use std::fs::File;
    use std::io::BufReader;
    use std::path::Path;
    use tokio_rustls::rustls;

    /// Load the certificate and private key from the file system and use them
    /// to create a rustls::ServerConfig.
    ///
    pub fn server_config(load_from: &Path) -> rustls::ServerConfig {
        let key = load_key(load_from.join("localhost.key"));
        let certs = load_certs(load_from.join("localhost.cert"));
        let mut config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .expect("tls config is invalid or missing");

        config.alpn_protocols = vec![b"h2".to_vec()];

        config
    }

    fn load_certs(path: impl AsRef<Path>) -> Vec<CertificateDer<'static>> {
        let file = File::open(path).expect("cert file is missing or corrupt");
        let mut reader = BufReader::new(file);

        rustls_pemfile::certs(&mut reader)
            .collect::<Result<_, _>>()
            .expect("cert chain is missing or invalid")
    }

    fn load_key(path: impl AsRef<Path>) -> PrivateKeyDer<'static> {
        File::open(path)
            .and_then(|file| rustls_pemfile::private_key(&mut BufReader::new(file)))
            .expect("failed to load private key")
            .expect("private key is missing or invalid")
    }
}
