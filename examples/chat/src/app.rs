use bb8::{Pool, PooledConnection};
use cookie::{Cookie, Key, SameSite};
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use time::OffsetDateTime;
use via::{Response, deny};
use zeroize::Zeroizing;

use crate::Request;
use crate::models::User;
use crate::util::{Authentication, Identity, Unauthorized};

const DATABASE_URL: &str = "VIA_SECRET_KEY";
const SESSION_SECRET: &str = "VIA_SECRET_KEY";

pub const SESSION: &str = "via-chat-session";

pub type Postgres = AsyncDieselConnectionManager<AsyncPgConnection>;
pub type Connection<'a> = PooledConnection<'a, Postgres>;

pub struct Unicorn {
    database: Pool<Postgres>,
    signer: Key,
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
        let token = Identity::new(user.id());

        // Build a response containing the authenticated user.
        let mut response = Response::build().status(201).data(user)?;

        // Add a session cookie containing token to the response cookies.
        self.refresh(&token, &mut response);

        Ok(response)
    }

    fn logout(&self, response: &mut Response) {
        response
            .cookies_mut()
            .signed_mut(&self.signer)
            .remove(Cookie::build(SESSION).path("/").build());
    }

    fn refresh(&self, identity: &Identity, response: &mut Response) {
        response
            .cookies_mut()
            .signed_mut(&self.signer)
            .add(make_session_cookie(identity.encode()));
    }

    fn restore(&self, request: &Request) -> Result<Identity, Unauthorized> {
        request
            .cookies()
            .signed(&self.signer)
            .get(SESSION)
            .ok_or(Unauthorized)
            .and_then(|cookie| cookie.value().parse())
    }
}
