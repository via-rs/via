use cookie::{Cookie, Key, SameSite};
use std::env;
use std::process::ExitCode;
use via::{Cookies, Error, Response, Server};

type Request = via::Request<CookiesExample>;
type Next = via::Next<CookiesExample>;

/// A struct used to store application app.
///
struct CookiesExample {
    /// The secret key used to sign, verify, and optionally encrypt cookies. The
    /// value of this key should be kept secret and changed periodically.
    ///
    secret: Key,
}

/// Increments the value of the "counter" counter to the console. Returns a
/// response with a message confirming the operation was successful.
///
async fn counter(request: Request, next: Next) -> via::Result {
    use time::Duration;

    // Clone `request.app()` so we can use the secret key after ownership of
    // request is passed to the next middleware.
    let app = request.app_owned();

    // Get the value of the "counter" cookie from the request before passing
    // ownership of the request to the next middleware. In this example, we are
    // using the signed cookie jar to store and retrieve the "counter" cookie.
    let mut counter = request
        .envelope()
        .cookies()
        .private(&app.secret)
        .get("counter")
        .map_or(Ok(0i32), |cookie| cookie.value().parse())?;

    // Call the next middleware to get the response.
    let mut response = next.call(request).await?;

    // Increment the value of the visit counter.
    counter += 1;

    // Print the number of times the user has visited the site to stdout.
    println!("User has visited {} times.", counter);

    // If the response status code is in 200..=299, update the counter cookie.
    if response.status().is_success() {
        response.cookies_mut().private_mut(&app.secret).add(
            Cookie::build(("counter", counter.to_string()))
                .http_only(true)
                .max_age(Duration::hours(1))
                .path("/")
                .same_site(SameSite::Strict)
                .secure(true),
        );
    }

    // Return the response.
    Ok(response)
}

/// Responds with a greeting message with the name provided in the request uri
/// path.
///
async fn greet(request: Request, _: Next) -> via::Result {
    // Get a reference to the path parameter `name` from the request uri.
    let name = request.param("name").decode().into_result()?;

    // Send a plain text response with our greeting message.
    Response::build().text(format!("Hello, {}!", name))
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    // Load the environment variables from the ".env" file. This is where we
    // keep the secret key in development. In production, you may want to
    // configure the secret key using a different method. For example, using
    // a parameter store or secret manager to set the value in the environment
    // at the time of deployment.
    //
    dotenvy::dotenv()?;

    // Create a new application.
    let mut app = via::app(CookiesExample {
        secret: env::var("VIA_SECRET_KEY")
            .map(|secret| secret.as_bytes().try_into())
            .expect("missing required env var: VIA_SECRET_KEY")
            .expect("unexpected end of input while parsing VIA_SECRET_KEY"),
    });

    // The CookieParser middleware can be added at any depth of the route tree.
    // In this example, we add it to the root of the app. This means that every
    // request will pass through the CookieParser middleware.
    app.uses(Cookies::new().allow("counter"));

    // Add the count_visits middleware to the app at "/".
    app.uses(counter);

    // Add a route that responds with a greeting message.
    app.route("/hello/:name").to(via::get(greet));

    Server::new(app).listen(("127.0.0.1", 8080)).await
}
