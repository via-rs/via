macro_rules! log {
    ($level:tt($name:ident), $fmt:literal $($arg:tt)*) => {
        log!($level($name = 0), $fmt $($arg)*)
    };
    ($level:tt($name:ident = $indent:expr), $fmt:literal $($arg:tt)*) => {
        #[cfg(debug_assertions)]
        eprintln!(
            "{:indent$}{}({}): {}",
            "",
            stringify!($level),
            stringify!($name),
            format_args!($fmt $($arg)*),
            indent = $indent * 2,
        )
    };
}

mod app;
mod models;
mod routes;
mod schema;
mod util;

use std::process::ExitCode;
use via::guard::{self, media, method};
use via::{Response, Server, cookies, rescue, router};

use app::Unicorn;
use routes::auth::{login, logout, me};
use routes::{channels, reactions, threads, users};
use util::session::{self, auth_required, authenticate};

type Request = via::Request<Unicorn>;
type Next = via::Next<Unicorn>;

#[tokio::main]
async fn main() -> via::Result<ExitCode> {
    // Setup our chat service, "Unicorn".
    let mut app = via::app(Unicorn::new().await?);

    // A temporary workaround before we implement CORS.
    app.route("/", via::get(async |_, _| Response::build().finish()));

    // If an error occurs, respond with JSON.
    app.middleware(rescue::json().build());

    // First, validate the request. Then, apply the cookies middleware.
    // The session cookie is managed by Via only if the request is valid.
    app.middleware(guard::flat_map(
        guard::content!(media::json()),
        cookies([app::SESSION]),
    ));

    // The /auth namespace.
    app.push("/auth").map(|path| {
        path.assign(via::post(login).delete(logout))
            .route("/me", via::get(me));
    });

    // The /api namespace.
    let mut path = app.push("/api");

    // Initialize the active user session.
    path.middleware(via::before(
        // Restore an identity token from the session cookie.
        app::restore_session,
        // If the request is read only or the session was verified in the
        // past hour, skip verifying that the user still has an account.
        //
        // This is safe so long as your app is the authoritative source of
        // truth for the session and you properly end websocket sessions if
        // the user deletes their account.
        guard::filter(
            guard::or((method::is_mutation(), session::needs_verified())),
            app::verify_session,
        ),
    ));

    // The /api/chat route.
    #[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
    path.route("/chat", via::get(authenticate(via::ws(routes::chat))));

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
