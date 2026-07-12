use serde::{Deserialize, Serialize};
use std::process::ExitCode;
use via::guard::{self, media};
use via::{Next, Request, Response, Router, Server, rescue};

#[derive(Debug, Deserialize, Serialize)]
struct Document<T> {
    data: T,
}

#[derive(Debug, Deserialize, Serialize)]
struct Greeting {
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Hello {
    name: String,
}

async fn hello(request: Request, _: Next) -> via::Result {
    use via::request::Payload;

    // A future that resolves with the frames that compose the request body.
    let (body, _app) = request.into_future();

    // Aggregate the frames of the request body and then deserialize a Document<Hello>.
    let hello: Document<Hello> = body.timeout_after_secs(5).await?.json()?;

    // Send a JSON response with our greeting message.
    Response::build().json(&Document {
        data: Greeting {
            message: format!("Hello, {}!", hello.data.name),
        },
    })
}

#[tokio::main]
async fn main() -> via::Result<ExitCode> {
    // Define the routes that our application responds to.
    let router = Router::new(|mut home| {
        // Errors that occur further down the stack generate a JSON response.
        home.middleware(rescue::json().build());

        // If the client does not speak JSON, deny the request.
        home.middleware(guard::barrier(guard::content!(media::json())));

        // Start defining descendants of "/".
        let mut path = home.prefix();

        // Define a route that listens on /hello/:name.
        path.route("/hello", via::post(hello).or_deny());
    });

    // Start listening at http://localhost:8080/ for incoming requests.
    Server::new(router, ()).listen(("127.0.0.1", 8080)).await
}
