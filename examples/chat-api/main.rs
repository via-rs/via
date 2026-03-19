mod routes;

use std::process::ExitCode;
use via::{Error, Guard, Request, Server, rest};

use routes::orgs;

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let mut app = via::app(());

    // app.uses(Cookies::new().allow(session::COOKIE));

    let mut api = app.route("api");

    // api.uses(Rescue::with(util::error_sanitizer));

    api.route("auth").scope(|auth| {
        use routes::auth::{login, logout, me};

        auth.index().to(via::delete(logout).post(login));
        auth.route("_me").to(via::get(me));
    });

    let mut org = api.resource(rest!(orgs, ":org-id"));

    // org.uses(session::restore);

    org.route("channels").scope(|channels| {
        use routes::channels::authorization;
        use routes::{reactions, threads};

        let (collection, member) = rest!(routes::channels);

        // Any request to subequently defined routes must be authenticated.
        channels.uses(Guard::new(|_: &Request| Ok(())));

        // Define create and index on /api/channels.
        //
        // These are commonly referred to as "collection" routes because they
        // operate on a collection of a resource.
        channels.index().to(collection);

        let mut channel = channels.route(":channel-id");

        // If a user tries to perform an action on a channel or one of it's
        // dependencies, they must be the owner of the resource or have
        // sufficent permission to perform the requested action.
        //
        // Including this middleware before anything else in the channel module
        // enforces that the `Ability` and `Subscriber` extension traits are
        // valid as long as they are visible in the type system.
        //
        // This is where seperation of concerns intersects with the uri path
        // and the API contract defined in `channels::authorization`.
        channel.uses(authorization);

        // Route /api/:org-id/channels/:channel-id to the member actions in the
        // `routes::channels` module. These include destroy, show, and update.
        //
        // Then, take a mutable reference to the ./:channel-id route and
        // continue defining the dependencies of an individual channel
        // resource.
        channel.to(member).scope(|channel| {
            let mut thread = channel.resource(rest!(threads, ":thread-id"));
            thread.resource(rest!(reactions, ":reaction-id"));

            let mut reply = thread.resource(rest!(threads, ":reply-id", "replies"));
            reply.resource(rest!(reactions, ":reaction-id"));
        });
    });

    org.route("chat").scope(|chat| {
        // Any request to subequently defined routes must be authenticated.
        chat.uses(Guard::new(|_: &Request| Ok(())));

        // Upgrade to a websocket and start chatting.
        // chat.index().to(via::ws(routes::chat));
    });

    org.route("users").scope(|users| {
        use routes::users::{create, index};

        let member = rest!(routes::users as member);

        // Creating an account does not require authentication.
        users.index().to(via::post(create));

        // Any request to subequently defined routes must be authenticated.
        users.uses(Guard::new(|_: &Request| Ok(())));

        users.index().to(via::get(index));
        users.route(":user-id").to(member);
    });

    // Start listening at http://localhost:8080 for incoming requests.
    Server::new(app).listen(("127.0.0.1", 8080)).await
}
