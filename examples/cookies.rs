use cookie::{Cookie, Key, SameSite};
use std::process::ExitCode;
use std::time::Duration;
use via::{Error, Next, Request, Response, ResultExt, Server};

struct Unicorn {
    secret: Key,
}

async fn hello(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
    let app = request.app_owned();

    // Get the value of the "counter" cookie from the request before passing
    // ownership of the request to the next middleware. In this example, we are
    // using the signed cookie jar to store and retrieve the "counter" cookie.
    let mut counter = request
        .cookies()
        .signed(&app.secret)
        .get("counter")
        .map_or(Ok(0i32), |cookie| cookie.value().parse())?;

    // Increment the value of the visit counter.
    counter += 1;

    let name = request.param("name").percent_decode().into_result()?;
    let name = name.as_ref();

    // Print the number of times the user has visited the site to stdout.
    println!("{} has visited {} times.", name, counter);

    // Send a plain text response with our greeting message.
    let mut response = Response::build().text(format!("Hello, {}!", name))?;

    response.cookies_mut().signed_mut(&app.secret).add(
        Cookie::build(("counter", counter.to_string()))
            .http_only(true)
            .path("/")
            .max_age(Duration::from_hours(24).try_into()?)
            .same_site(SameSite::Strict)
            .secure(true),
    );

    Ok(response)
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    // Create a new application.
    let mut app = via::app(Unicorn {
        // secret: std::env::var("VIA_SECRET_KEY")
        //     .map(|secret| secret.as_bytes().try_into())
        //     .expect("missing required env var: VIA_SECRET_KEY")
        //     .expect("unexpected end of input while parsing VIA_SECRET_KEY"),
        secret: Key::generate(),
    });

    // The Cookies middleware can be added at any depth of the route tree.
    // In this example, we add it at the root path of our application. This
    // enables cookie support for all of the subsequently defined middlewares.
    app.middleware(via::cookies(["counter"]));

    // Add a route that responds with a greeting message.
    app.route("/hello/:name").to(via::get(hello));

    Server::new(app).listen(("127.0.0.1", 8080)).await
}
