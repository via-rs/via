mod chat;
mod models;
mod routes;
mod schema;
mod util;

use std::process::ExitCode;
use via::error::{Error, Rescue};
use via::{Cookies, Guard, Server};

use chat::Chat;
use routes::{channel, channels, chat, home, users};
use util::session::{self, Session};

type Request = via::Request<Chat>;
type Next = via::Next<Chat>;

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    dotenvy::dotenv()?;

    let mut app = via::app({
        let pool = chat::establish_pg_connection().await;
        let secret = chat::load_session_secret();

        Chat::new(pool, secret)
    });

    app.uses(Cookies::new().allow(session::COOKIE));

    app.route("/").to(via::get(home));

    let mut api = app.route("/api");

    api.uses(Rescue::with(util::error_sanitizer));
    api.uses(session::restore);

    api.route("/auth").scope(|path| {
        use routes::auth::{login, logout, me};

        path.route("/").to(via::delete(logout).post(login));
        path.route("/_me").to(via::get(me));
    });

    api.route("/channels").scope(|path| {
        // Any request to /api/channels requires authentication.
        path.uses(Guard::new(Request::authenticate));

        // Define create and index on /api/channels.
        //
        // These are commonly referred to as "collection" routes because they
        // operate on a collection of a resource.
        path.route("/").to(via::rest!(channels as collection));

        // Bind `path` to /api/channels/:channel-id.
        let mut path = path.route("/:channel-id");

        // If a user tries to perform an action on a channel or one of it's
        // dependencies, they must be the owner of the resource or have
        // sufficent permission to perform the requested action.
        //
        // Including this middleware before anything else in the channel module
        // enforces that the `Ability` and `Subscriber` extension traits are
        // valid as long as they are visible in the type system.
        //
        // This is where seperation of concerns intersects with the uri path
        // and the API contract defined in `channel::authorization`.
        path.uses(channel::authorization);

        // Define show, update, and destroy on /api/channels/:channel-id.
        //
        // These are commonly referred to as "member" actions because they
        // operate on a single existing resource.
        path.route("/").to(via::rest!(channel as member));

        // Continue defining the dependencies of a channel.

        path.route("/reactions").scope(|reactions| {
            let (collection, member) = via::rest!(channel::reactions);

            reactions.route("/").to(collection);
            reactions.route("/:reaction-id").to(member);
        });

        path.route("/subscriptions").scope(|subscriptions| {
            let (collection, member) = via::rest!(channel::subscriptions);

            subscriptions.route("/").to(collection);
            subscriptions.route("/:subscription-id").to(member);
        });

        path.route("/threads").scope(|threads| {
            use channel::reactions::create as react;

            let mut thread = {
                let (collection, member) = via::rest!(channel::threads);

                threads.route("/").to(collection);
                threads.route("/:thread-id").to(member)
            };

            thread.route("/reactions").to(via::post(react));

            thread.route("/replies").scope(|replies| {
                let (collection, member) = via::rest!(channel::threads);
                replies.route("/").to(collection);

                let mut reply = replies.route("/:reply-id").to(member);
                reply.route("/reactions").to(via::post(react));
            });
        });
    });

    api.route("/chat").scope(|path| {
        // Any request to /api/chat requires authentication.
        path.uses(Guard::new(Request::authenticate));

        // Upgrade to a websocket and start chatting.
        path.route("/").to(via::ws(chat));
    });

    api.route("/users").scope(|path| {
        // Creating an account does not require authentication.
        path.route("/").to(via::post(users::create));

        // Subsequent requests to /api/users requires authentication.
        path.uses(Guard::new(Request::authenticate));

        path.route("/").to(via::get(users::index));
        path.route("/:user-id").to(via::rest!(users as member));
    });

    // Start listening at http://localhost:8080 for incoming requests.
    Server::new(app).listen(("127.0.0.1", 8080)).await
}
