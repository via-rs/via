use base64::engine::Engine;
use bb8::{Pool, PooledConnection};
use cookie::{Cookie, Key, SameSite};
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use time::OffsetDateTime;
use via::error::{Catch, Propagate};
use via::guard::{Project, error::UnknownExtension};
use via::{Response, deny};
use zeroize::Zeroizing;

use crate::Request;
use crate::models::User;
use crate::util::session::{CODEC, SESSION};
use crate::util::{Authentication, Id, Identity, Session, Unauthorized};

const DATABASE_URL: &str = "VIA_SECRET_KEY";
const SESSION_SECRET: &str = "VIA_SECRET_KEY";

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

fn make_session_cookie(value: String) -> Cookie<'static> {
    use time::Duration;

    Cookie::build((SESSION, value))
        .http_only(true)
        .same_site(SameSite::Strict)
        .expires(OffsetDateTime::now_utc() + Duration::weeks(2))
        .secure(true)
        .path("/")
        .build()
}

fn require_env(name: &str) -> via::Result<Zeroizing<String>> {
    dotenvy::var(name).map_or_else(
        |_| deny!(500, "missing required env variable: {}", name),
        |value| Ok(Zeroizing::new(value)),
    )
}

impl Unicorn {
    pub async fn new() -> via::Result<Self> {
        let secret = require_env(SESSION_SECRET)?;
        let manager = Postgres::new(&*require_env(DATABASE_URL)?);
        let Ok(pool) = Pool::builder().build(manager).await else {
            deny!(500, "database is unavailable");
        };

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

impl Authentication for Unicorn {
    fn login(&self, user: User) -> via::Result {
        // Create a Base64 encoded identity token for `user`.
        let identity = Identity::new(user.id());

        // Build a response containing the authenticated user.
        let mut response = Response::build().status(201).data(user)?;

        // Add a session cookie containing token to the response cookies.
        self.refresh(&identity, &mut response);

        Ok(response)
    }

    fn logout(&self, response: &mut Response) {
        response
            .cookies_mut()
            .signed_mut(&self.signer)
            .remove(Cookie::build(SESSION).path("/").build());
    }

    fn refresh(&self, identity: &Identity, response: &mut Response) {
        let token = CODEC.encode(identity.as_ref());

        response
            .cookies_mut()
            .signed_mut(&self.signer)
            .add(make_session_cookie(token));
    }
}

// impl Session for Envelope {
//     #[inline]
//     fn me(&self) -> Result<Id, Unauthorized> {
//         match self.extensions().get() {
//             Some(ProtectFromForgery(identity)) => identity.id(),
//             None => Err(Unauthorized),
//         }
//     }
// }

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
