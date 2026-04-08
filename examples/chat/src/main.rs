mod database;
mod routes;
mod util;

use cookie::Key;
use std::process::ExitCode;
use via::{Error, Server, raise, rest};

use database::Database;
use routes::{auth, channels, reactions, threads, users};
use util::session::{self, Session};

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

    let mut api = app.route("/api");

    api.middleware(via::rescue(|sanitizer| sanitizer.use_json()));
    api.middleware(via::cookies([session::COOKIE]));
    api.middleware(session::restore);

    api.route("/auth").scope(|auth| {
        auth.index().to(via::delete(auth::logout).post(auth::login));
        auth.route("/_me").to(via::get(auth::me));
    });

    api.route("/channels").scope(|channels| {
        let (collection, member) = rest!(channels);

        // Define create and index on /api/channels.
        //
        // These are commonly referred to as "collection" routes because they
        // operate on a collection of a resource.
        channels.index().to(collection);

        channels.route("/:channel-id").scope(|channel| {
            let threads = rest!(threads, ":thread-id");
            let replies = rest!(threads, ":reply-id", "replies");

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

            // Route /api/:org-id/channels/:channel-id to the member actions in
            // the `routes::channels` module. These include destroy, show, and
            // update.
            channel.index().to(member);

            // Continue defining the dependencies of a channel.

            channel.resource(threads).scope(|thread| {
                thread.resource(rest!(reactions, ":reaction-id"));
                thread.resource(replies).scope(|reply| {
                    reply.resource(rest!(reactions, ":reaction-id"));
                });
            });
        });
    });

    api.route("/chat").scope(|chat| {
        // Any request to subequently defined routes must be authenticated.
        chat.middleware(via::guard(
            || raise!(401, message = "unauthorized"),
            |request: &Request| request.session().is_ok(),
        ));

        // Upgrade to a websocket and start chatting.
        chat.index().to(via::ws(routes::chat));
    });

    api.route("/users").scope(|users| {
        // Creating an account does not require authentication.
        users.index().to(via::post(users::create));

        users.index().to(via::get(users::index));
        users.route("/:user-id").to(rest!(users as member));
    });

    // Start listening at http://localhost:8080 for incoming requests.
    Server::new(app).listen(("127.0.0.1", 8080)).await
}
