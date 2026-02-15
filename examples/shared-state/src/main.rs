use std::fmt::Write;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use via::{Error, Next, Request, Response, Server};

#[derive(Default)]
struct Counter {
    /// The number of responses with a status code in `100..=299`.
    successes: Arc<AtomicU32>,

    /// The number of responses with a status code in `400..=599`.
    errors: Arc<AtomicU32>,
}

/// A responder that will return the total number of `successes` and `errors`
/// in the `Counter` app.
async fn totals(request: Request<Counter>, _: Next<Counter>) -> via::Result {
    let app = request.app();

    // Load the current value of `errors` from the atomic integer.
    let errors = app.errors.load(Ordering::Relaxed);

    // Load the current value of `successes` from the atomic integer.
    let successes = app.successes.load(Ordering::Relaxed);

    // Create a new string to hold the message. Since we want the message
    // to be multiple lines, we'll use `writeln!` instead of `format!`.
    let mut message = String::new();

    writeln!(&mut message, "Errors: {}", errors)?;
    writeln!(&mut message, "Sucesses: {}", successes)?;

    // Return a string with the total number of `errors` and `successes`.
    Response::build().text(message)
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let mut app = via::app(Counter::default());

    // Add the `counter` middleware to the application. Since we are not
    // specifying an endpoint with the `at` method, this middleware will
    // be applied to all requests.
    app.uses(async |request: Request<Counter>, next: Next<Counter>| {
        // App is used after the response is generated.
        let app = request.app_owned();

        // Call the next middleware in the app and await the response.
        let response = next.call(request).await?;

        if response.status().is_client_error() || response.status().is_server_error() {
            // The response status code is in 400..=599.
            app.errors.fetch_add(1, Ordering::Relaxed);
        } else {
            // The response status code is in 100..=299.
            app.successes.fetch_add(1, Ordering::Relaxed);
        }

        Ok(response)
    });

    // Add the `totals` responder to the endpoint GET /totals.
    app.route("/totals").to(via::get(totals));

    Server::new(app).listen(("127.0.0.1", 8080)).await
}
