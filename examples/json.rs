use http::header::ACCEPT;
use serde::{Deserialize, Serialize};
use std::process::ExitCode;
use via::guard::{header, method};
use via::{Error, Next, Payload, Request, Response, Server, deny, guard, rescue};

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
    // A future that resolves with the frames that compose the request body.
    let (future, _app) = request.into_future();

    // Aggregate the frames of the request body and then deserialize a Document<Hello>.
    let body: Document<Hello> = future.await?.json()?;

    // Send a JSON response with our greeting message.
    Response::build().json(&Document {
        data: Greeting {
            message: format!("Hello, {}!", &body.data.name),
        },
    })
}

fn content_negotiation_failed(error: guard::ErrorKind) -> Error {
    if let guard::ErrorKind::Header(ACCEPT) = error {
        deny(406, "unsupported response format.")
    } else {
        deny(415, "unsupported media type.")
    }
}

#[tokio::main]
async fn main() -> via::Result<ExitCode> {
    let mut app = via::app(());

    // Errors that occur further down the stack generate a JSON response.
    app.middleware(rescue(|error| error.use_json()));

    // Content negotiation
    app.middleware(guard(
        content_negotiation_failed,
        guard::and((
            header::accept::json().optional(),
            guard::when(method::is_mutation(), header::content_type::json()),
        )),
    ));

    // Define a route that responds to POST /hello.
    app.route("/hello").to(via::post(hello).or_deny());

    Server::new(app).listen(("127.0.0.1", 8080)).await
}
