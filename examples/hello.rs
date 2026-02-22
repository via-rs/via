use std::process::ExitCode;
use via::{Error, Next, Request, Response, Server};

async fn hello(request: Request, _: Next) -> via::Result {
    // Get a reference to the path parameter `name` from the request uri.
    let name = request.param("name").decode().into_result()?;

    // Send a plain text response with our greeting message.
    Response::build().text(format!("Hello, {}!", name.as_ref()))
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let mut app = via::app(());

    // Define a route that listens on /hello/:name.
    app.route("/hello/:name").to(via::get(hello));

    Server::new(app).listen(("127.0.0.1", 8080)).await
}
