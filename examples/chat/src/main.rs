mod database;
mod routes;
mod util;

use cookie::Key;
use std::process::ExitCode;
use via::guard::{self, media, method};
use via::{Error, Server, cookies, rescue, router};

use database::Database;
use routes::auth::{login, logout, me};
use routes::{channels, chat, reactions, threads, users};
use util::session::{self, auth_required, authenticate};

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
    // Create our chat application, "Unicorn".
    let mut app = via::app(Unicorn {
        database: Database::new()?,
        secret: Key::generate(),
        // secret: std::env::var("VIA_SECRET_KEY")
        //     .map(|secret| secret.as_bytes().try_into())
        //     .expect("missing required env var: VIA_SECRET_KEY")
        //     .expect("unexpected end of input while parsing VIA_SECRET_KEY"),
    });

    // The /api namespace.
    let mut path = app.push("/api");

    // If an error occurs, respond with JSON.
    path.middleware(rescue(|sanitizer| sanitizer.use_json()));

    // Parse and track changes that are made to the session cookie.
    path.middleware(cookies([session::COOKIE]));

    // Content negotiation, session restoration, and verification.
    path.middleware(guard::flat_map(
        // Confirm that the client speaks JSON.
        guard::content!(media::json()),
        // Then, initialize the active user session.
        via::before(
            // Restore an identity token from the session cookie.
            session::restore,
            // If the request is read only or the session was verified in the
            // past hour, skip verifying that the user still has an account.
            //
            // This is safe so long as your app is the authoritative source of
            // truth for the session and you properly end websocket sessions if
            // the user deletes their account.
            guard::filter(
                guard::or((method::is_mutation(), session::needs_verified())),
                session::verify(),
            ),
        ),
    ));

    // The /api/auth namespace.
    path.route("/auth", via::delete(logout).post(login))
        .route("/me", via::get(me));

    // The /api/chat route.
    #[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
    path.route("/chat", via::get(authenticate(via::ws(chat))));

    // The /api/channels resource.
    path.route("/channels", channels::collection(auth_required))
        .push("/:channel-id")
        // Resources nested within a channel pay the cost of an additional
        // middleware "hop" in exchange for combining the `auth_required`
        // predicate with authorization middleware that applies to every
        // descendant.
        //
        // Any request made to a route within the /api/channels/:channel-id
        // must come from an authenticated user with the minimum set of
        // permissions required to perform the requested action.
        .map(router::apply(authenticate(channels::authorization)))
        // This includes the actions in the channels "member" scope.
        .assign(channels::member())
        // The ./channels/:channel-id/threads resource
        .route("/threads", threads::collection())
        .route("/:thread-id", threads::member())
        // Threads have more than one descendant. The map function can be used
        // to define adjacent siblings in a new scope without having to bind
        // the path to a new variable.
        .map(|mut path| {
            // The ./:thread-id/reactions resource
            path.route("/reactions", reactions::collection())
                .route("/:reaction-id", reactions::member());

            // The ./:thread-id/replies resource
            path.route("/replies", threads::collection())
                .route("/:reply-id", threads::member())
                // The ./:thread-id/replies/:reply-id/reactions resource
                .route("/reactions", reactions::collection())
                .route("/:reaction-id", reactions::member());
        });

    // The /api/users resource.
    path.route("/users", users::collection(auth_required))
        .route("/:user-id", users::member(auth_required));

    // Start listening at http://localhost:8080 for incoming requests.
    Server::new(app).listen(("127.0.0.1", 8080)).await
}
