use std::process::ExitCode;
use via::{Next, Request, Response, ResultExt, Router, Server};

async fn hello(request: Request, _: Next) -> via::Result {
    // Get a reference to `name` from the request uri path.
    let name = request.param("name").percent_decode().into_result()?;

    // Send a plain text response with our greeting message.
    Response::build().text(format!("Hello, {}!", name.as_ref()))
}

#[tokio::main]
async fn main() -> via::Result<ExitCode> {
    // Define the routes that our application responds to.
    let router = Router::new(|home| {
        // Start defining descendants of "/".
        let mut path = home.prefix();

        // Define a route that listens on /hello/:name.
        path.route("/hello/:name", via::get(hello));
    });

    // Start listening at http://localhost:8080/ for incoming requests.
    Server::new(router, ()).listen(("127.0.0.1", 8080)).await
}
