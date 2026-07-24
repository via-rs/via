use std::thread::available_parallelism;

use base64::engine::Engine;
use bb8::{Pool, PooledConnection};
use cookie::{Cookie, Key, SameSite};
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use via::error::{Catch, Propagate};
use via::guard::{Project, error::UnknownExtension};
use via::{Response, deny};
use via_pubsub::{Pubsub, backend::Redis};
use zeroize::Zeroizing;

use crate::models::{ReactionWithUser, ThreadWithUser, User};
use crate::util::session::{CODEC, Identity, Unauthorized};
use crate::util::{Authenticator, Id, Session};
use crate::{Next, Request};

const DATABASE_URL: &str = "DATABASE_URL";
const REDIS_URL: &str = "REDIS_URL";

/// The redis channel namespace to which peer events are published.
const PUBSUB_SCOPE: &str = "unicorn";

// The signing key used to sign and verify peer events.
const PUBSUB_SIGNER: &str = "PUBSUB_SIGNER";

/// The schema version used to deserialize peer events.
const PUBSUB_VERSION: u32 = 1;

/// The signing key used to sign and verify session cookies.
const SESSION_SIGNER: &str = "SESSION_SIGNER";

/// The maximum size in bytes of chat message or reaction.
pub const MAX_EVENT_SIZE: usize = 8192;

/// The cookie name used to store an encoded identity token.
pub const SESSION: &str = "via-chat-session";

pub type Postgres = AsyncDieselConnectionManager<AsyncPgConnection>;
pub type Connection<'a> = PooledConnection<'a, Postgres>;

/// A change notification.
#[derive(Debug, Deserialize, Serialize)]
#[serde(content = "data", rename_all = "lowercase", tag = "type")]
pub enum Notification {
    Reaction(ReactionWithUser),
    Reply(ThreadWithUser),
}

/// Our billion dollar chat application.
/// This type defines the resources that are available to each request.
pub struct Unicorn {
    database: Pool<Postgres>,
    pubsub: Pubsub<Redis<Id, Notification>>,
    signer: Key,
}

/// Project the protected Identity extension for a guard predicate.
pub struct IdentityExtension;

/// A private extension type that ensures that the Identity extension can only
/// be accessed with the methods provided by this module.
#[derive(Clone)]
struct ProtectFromForgery(Identity);

/// Restores a session from a session cookie.
///
/// This function is intended to be used with `via::before`.
///
/// If there is not a session associated with the request, the subsequent
/// middleware is skipped.
//
// We implement this business logic in the same module as `Unicorn` so we can:
//   - Access the signer field without writing a public accessor
//   - Enforce that a session can only be accessed, created, or modified in
//     this file
pub fn restore_session(request: &mut Request) -> Result<(), Catch> {
    // Extract an identity token from the request's session cookie.
    let identity = request
        .cookies()
        .signed(&request.app().signer)
        .get(SESSION)
        .ok_or(Unauthorized)
        .and_then(|cookie| Identity::decode(cookie.value()))
        .or_continue()?;
    //   ^^^^^^^^^^^
    // If extraction fails, fall through to the next middleware.
    // Some routes do not require an authenticated user.

    // Insert the protected identity token into the request extensions.
    request
        .extensions_mut()
        .insert(ProtectFromForgery(identity));

    Ok(())
}

/// Verify that the active user still has an account.
pub async fn verify_session(mut request: Request, next: Next) -> via::Result {
    // Get the id of the active user from the session.
    let me = request.me()?;

    // If `Ok(())`, the account is valid.
    let result = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Execute the query.
        User::exists(&mut connection, me).await
    };

    // Verify the active user's account and create a fresh identity token.
    let identity = match result {
        // The account is valid, create a fresh identity token.
        Ok(_) => Identity::new(me),

        // The user does not exist, destroy the session.
        Err(error) if error.status() == StatusCode::UNAUTHORIZED => {
            // Build a response with the auth error as json.
            let mut response = Response::build()
                .status(StatusCode::UNAUTHORIZED)
                .errors(error)?;

            // Instruct the client to remove the session cookie.
            request.app().logout(&mut response);

            // Return an error response that removes the session.
            return Ok(response);
        }

        // Some other error occurred during verification.
        Err(error) => return Err(error),
    };

    // Swap the identity token in the extension with a clone of `identity`.
    if let Some(ProtectFromForgery(protected)) = request.extensions_mut().get_mut() {
        *protected = identity.clone();
    }

    // Get an owned handle to our application, Unicorn.
    let app = request.app_owned();

    // Await the response from the next middleware.
    let mut response = next.call(request).await?;

    // Update the session cookie with a new base64 encoded identity token.
    refresh_session(&mut response, &app.signer, identity);

    Ok(response)
}

/// Persist a session cookie that contains a base64 encoded identity token.
fn refresh_session(response: &mut Response, signer: &Key, identity: Identity) {
    // Create a base64 encoded string containing the identity token.
    let token = CODEC.encode(identity.as_ref());

    // Build an HttpOnly session cookie with the encoded identity token.
    let cookie = Cookie::build((SESSION, token))
        .http_only(true)
        .same_site(SameSite::Strict)
        .expires(OffsetDateTime::now_utc() + time::Duration::weeks(2))
        .secure(true)
        .path("/")
        .build();

    // Add the session cookie to the signed cookie jar to prevent tampering.
    response.cookies_mut().signed_mut(signer).add(cookie);
}

fn require_env(name: &str) -> via::Result<String> {
    dotenvy::var(name).or_else(|_| deny!(500, "missing required env variable: {}", name))
}

fn require_secret(name: &str) -> via::Result<Zeroizing<String>> {
    require_env(name).map(Zeroizing::new)
}

impl Unicorn {
    pub async fn new() -> via::Result<(usize, Self)> {
        // Get the suggested amount of parallelism from the environment.
        //
        // We use this value to determine the size of our connection pool, the
        // capacity of our PubSub channels, and the server connection margin.
        //
        // Deterministic resource usage is how we re-route load to other nodes
        // before connections start to queue in the kernel backlog on Linux.
        let num_workers = available_parallelism()?.get();

        // Establish a database connection and construct a connection pool.
        let database = {
            let connection_url = require_env(DATABASE_URL)?;
            let manager = Postgres::new(connection_url);
            let pool = Pool::builder()
                .max_size(num_workers as u32)
                //                    ^^^^^^
                // Cast is safe. 4294967295 processors would be nice though.
                .build(manager)
                .await?;

            // Checkout a connection to confirm we have a healthy connection.
            pool.get().await?;
            pool
        };

        // Establish a connection to redis a construct a pubsub client.
        let pubsub = {
            let url = require_env(REDIS_URL)?;

            Redis::builder(PUBSUB_SCOPE)
                .concurrency(num_workers)
                .max_event_size(MAX_EVENT_SIZE)
                .signing_key(require_secret(PUBSUB_SIGNER)?.as_bytes())
                //           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
                //     Signing key is dropped and zeroed before await.
                .version(PUBSUB_VERSION)
                .connect(&url)
                .await?
        };

        // Construct app with the handles to the aforementioned external
        // resources and a session signing key.
        let app = Self {
            database,
            pubsub,
            signer: Key::from(require_secret(SESSION_SIGNER)?.as_bytes()),
        };

        Ok((num_workers + 1, app))
        //  ^^^^^^^^^^^^^^^
        //
        // Add 1 to the number of workers to account for the additional fd used
        // by the PubSub redis client.
    }

    pub async fn database(&self) -> via::Result<Connection<'_>> {
        match self.database.get().await {
            Ok(connection) => Ok(connection),

            #[cfg(not(debug_assertions))]
            Err(_) => Err(via::err!(500, "internal server error")),

            #[cfg(debug_assertions)]
            Err(error) => {
                log!(error(database), "{}", &error);
                Err(via::err!(500, "internal server error"))
            }
        }
    }

    pub fn pubsub(&self) -> &Pubsub<Redis<Id, Notification>> {
        &self.pubsub
    }
}

impl Authenticator for Unicorn {
    fn login(&self, user: User) -> via::Result {
        // Create a new identity token for `user`.
        let identity = Identity::new(*diesel::Identifiable::id(&user));

        // Build a response containing `user` as json.
        let mut response = Response::build().status(StatusCode::CREATED).data(user)?;

        // Update the session cookie with the base64 encoded identity token.
        refresh_session(&mut response, &self.signer, identity);

        Ok(response)
    }

    fn logout(&self, response: &mut Response) {
        response
            .cookies_mut()
            .signed_mut(&self.signer)
            .add(Cookie::build(SESSION).path("/").removal().build());
    }
}

impl Session for Request {
    fn me(&self) -> Result<Id, Unauthorized> {
        match self.extensions().get() {
            Some(ProtectFromForgery(identity)) => identity.id(),
            None => Err(Unauthorized),
        }
    }
}

#[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
impl Session for via::ws::Request<Unicorn> {
    fn me(&self) -> Result<Id, Unauthorized> {
        match self.extensions().get() {
            Some(ProtectFromForgery(identity)) => identity.id(),
            None => Err(Unauthorized),
        }
    }
}

impl Project<Request> for IdentityExtension {
    type Output = Identity;
    type Error<'a> = UnknownExtension;

    fn project<'a, 'b>(
        &'a self,
        request: &'b Request,
    ) -> Result<&'b Self::Output, Self::Error<'a>> {
        match request.extensions().get() {
            Some(ProtectFromForgery(identity)) => Ok(identity),
            None => Err(UnknownExtension),
        }
    }
}
