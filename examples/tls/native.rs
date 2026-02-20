use native_tls::Identity;
use std::path::{Path, PathBuf};
use std::{fs, process::ExitCode};
use via::{Error, Next, Request, Response, Server};

const P12_PASSWORD: &str = "TLS_PKCS_PASSWORD";

async fn hello(request: Request, _: Next) -> via::Result {
    // Get a reference to the path parameter `name` from the request uri.
    let name = request.param("name").decode().into_result()?;

    // Send a plain text response with our greeting message.
    Response::build().text(format!("Hello, {}! (via TLS)", name))
}

fn load_pkcs12() -> Result<Identity, Error> {
    let path_to_this_file = PathBuf::from(file!());
    let tls_examples_dir = path_to_this_file.parent().unwrap();

    let identity = {
        let p12_path = tls_examples_dir.join("localhost.p12");
        fs::read(&p12_path).unwrap_or_else(|_| {
            panic!("failed to load pkcs#12 file from: {:?}", p12_path);
        })
    };

    let password = {
        let env_path = tls_examples_dir.join(".env");
        dotenvy::from_path_iter(env_path.as_path())?
            .find_map(|result| match result {
                Ok((key, value)) if key == P12_PASSWORD => Some(value),
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "missing required env var \"{}\" in {:?}",
                    P12_PASSWORD, env_path
                );
            })
    };

    Ok(Identity::from_pkcs12(&identity, &password)?)
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    // Make sure that our TLS config is present and valid before we proceed.
    let tls_config = load_pkcs12()?;
    let mut app = via::app(());

    // Add our hello responder to the endpoint /hello/:name.
    app.route("/hello/:name").to(via::get(hello));

    Server::new(app)
        .listen_native_tls(("127.0.0.1", 8080), tls_config)
        .await
}
