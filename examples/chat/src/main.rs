mod database;
mod routes;
mod util;

use cookie::Key;
use std::process::ExitCode;
use via::error::{Error, rescue};
use via::{Server, collection, cookies, guard, member, rest};

use database::Database;
use routes::auth::{login, logout, me};
use routes::{channels, reactions, threads, users};
use util::session::{self, is_authenticated, unauthorized};

type Request = via::Request<Unicorn>;
type Next = via::Next<Unicorn>;

struct Unicorn {
    database: Database,
    secret: Key,
}

impl Unicorn {
    #[inline]
    fn database(&self) -> &Database {
        &self.database
    }

    #[inline]
    fn secret(&self) -> &Key {
        &self.secret
    }
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let mut app = via::app(Unicorn {
        database: Database::new()?,
        secret: Key::generate(),
        // secret: std::env::var("VIA_SECRET_KEY")
        //     .map(|secret| secret.as_bytes().try_into())
        //     .expect("missing required env var: VIA_SECRET_KEY")
        //     .expect("unexpected end of input while parsing VIA_SECRET_KEY"),
    });

    // The /api namespace.
    let mut api = app.route("api");

    // If an error occurs, respond with JSON.
    api.middleware(rescue(|sanitizer| sanitizer.use_json()));

    // Parse and track changes that are made to the session cookie.
    api.middleware(cookies([session::COOKIE]));

    // Restore the user's session if it exists.
    api.middleware(session::restore);

    // The /api/auth namespace.
    let mut auth = api.route("auth");

    auth.index().to(via::delete(logout).post(login));
    auth.route("me").to(via::get(me));

    // The /api/chat route.
    #[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
    api.route("chat").scope(|chat| {
        // Opening a web socket requires an authenticated user.
        chat.middleware(guard(unauthorized, is_authenticated));
        chat.index().to(via::get(via::ws(routes::chat)));
    });

    // The /api/users resource.
    let mut users = api.route("users").to(via::post(users::create));
    //                                    ^^^^^^^^^^^^^^^^^^^^^^^^
    // Creating an account does not require authentication.
    // However, subsquenet request to users resource must be authenticated.
    users.middleware(guard(unauthorized, is_authenticated));

    users.index().to(via::get(users::index));
    users.route(":user-id").to(member!(users));

    // The /api/channels resource.
    let mut channels = api.route("channels");

    // All requests to the channels resource must be authenticated.
    channels.middleware(guard(unauthorized, is_authenticated));

    channels.index().to(collection!(channels));
    channels.route(":channel-id").scope(|channel| {
        // If a user tries to perform an action on a channel or one of it's
        // dependencies, they must be the owner of the resource or have
        // sufficent permission to perform the requested action.
        //
        // Including this middleware before anything else in the channel
        // module enforces that the `Ability` and `Subscriber` extension
        // traits are valid as long as they are visible in the type system.
        //
        // This is where seperation of concerns intersects with the uri path
        // and the API contract defined in `channels::authorization`.
        channel.middleware(channels::authorization);

        channel.index().to(member!(channels));

        // Continue defining the dependencies of a channel.

        let mut thread = channel.resource(rest!(threads, ":thread-id"));
        let mut reply = thread.resource(rest!(threads, ":reply-id", "replies"));

        reply.resource(rest!(reactions, ":reaction-id"));
        thread.resource(rest!(reactions, ":reaction-id"));
    });

    // Start listening at http://localhost:8080 for incoming requests.
    Server::new(app).listen(("127.0.0.1", 8080)).await
}
