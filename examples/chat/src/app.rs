use base64::engine::Engine;
use bb8::{Pool, PooledConnection};
use cookie::{Cookie, Key, SameSite};
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use http::StatusCode;
use time::OffsetDateTime;
use via::error::{Catch, Propagate};
use via::guard::{Project, error::UnknownExtension};
use via::{Response, deny};
use zeroize::Zeroizing;

use crate::models::User;
use crate::util::session::{CODEC, Identity, Unauthorized};
use crate::util::{Authenticator, Id, Session};
use crate::{Next, Request};

const DATABASE_URL: &str = "DATABASE_URL";
const SESSION_SECRET: &str = "VIA_SECRET_KEY";

/// The cookie name used to store an encoded identity token.
pub const SESSION: &str = "via-chat-session";

pub type Postgres = AsyncDieselConnectionManager<AsyncPgConnection>;
pub type Connection<'a> = PooledConnection<'a, Postgres>;

/// Our billion dollar chat application.
/// This type defines the resources that are available to each request.
pub struct Unicorn {
    database: Pool<Postgres>,
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
        .and_then(|cookie| cookie.value().parse())
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
pub async fn verify_session(request: Request, next: Next) -> via::Result {
    // Get the id of the active user from the session.
    let me = request.me()?;

    {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Confirm that the user has an active account.
        if let Err(error) = User::exists(&mut connection, me).await {
            // If the user does not exist, destroy the session.
            return if error.status() == StatusCode::UNAUTHORIZED {
                // Build a response with the auth error as json.
                let mut response = Response::build()
                    .status(StatusCode::UNAUTHORIZED)
                    .errors(error)?;

                // Instruct the client to remove the session cookie.
                request.app().logout(&mut response);

                Ok(response)
            } else {
                Err(error)
            };
        }
    }

    // Get an owned handle to our application, Unicorn.
    let app = request.app_owned();

    // Await the response from the next middleware.
    let mut response = next.call(request).await?;

    // Update the session cookie with a new base64 encoded identity token.
    refresh_session(&mut response, &app.signer, Identity::new(&me));

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

fn require_env(name: &str) -> via::Result<Zeroizing<String>> {
    dotenvy::var(name).map_or_else(
        |_| deny!(500, "missing required env variable: {}", name),
        |value| Ok(Zeroizing::new(value)),
    )
}

impl Unicorn {
    pub async fn new() -> via::Result<Self> {
        let manager = Postgres::new(&*require_env(DATABASE_URL)?);
        let secret = require_env(SESSION_SECRET)?;
        let pool = Pool::builder().build(manager).await?;

        Ok(Self {
            database: pool,
            signer: Key::from(secret.as_bytes()),
        })
    }

    pub async fn database(&self) -> via::Result<Connection<'_>> {
        self.database.get().await.map_err(|error| {
            log!(error(database), "{}", &error);
            via::err!(500, "internal server error")
        })
    }
}

impl Authenticator for Unicorn {
    fn login(&self, user: User) -> via::Result {
        // Create a new identity token for `user`.
        let identity = Identity::new(diesel::Identifiable::id(&user));

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
