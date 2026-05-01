use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use cookie::{Cookie, Key, SameSite};
use http::StatusCode;
use std::str::FromStr;
use time::{Duration, OffsetDateTime};
use via::{Error, Middleware, Response, deny, err, guard};

use crate::database::models::User;
use crate::database::{Database, Id};
use crate::{Next, Request, Unicorn};

pub const COOKIE: &str = "via-chat-session";

const EXPIRES_AT: usize = 8;
const TOKEN_LEN: usize = 16;

pub type IsAuthenticated = guard::MapErr<fn(()) -> Unauthorized, fn(&Request) -> bool>;

pub trait Authenticate {
    fn authenticate(&mut self, secret: &Key, user: Option<Identity>);
}

pub trait Session {
    fn session(&self) -> Option<&Identity>;
    async fn user(&self) -> Result<User, Error>;
}

#[derive(Clone, PartialEq)]
pub struct Identity([u8; 16]);

pub struct RestoreSession;

pub struct Unauthorized;

#[derive(Clone)]
struct ProtectFromForgery(Identity);

pub fn is_authenticated() -> IsAuthenticated {
    guard::map_err(
        |_| Unauthorized,
        |request| {
            request.session().is_some_and(|id| {
                let now = OffsetDateTime::now_utc().unix_timestamp();
                id.expires_at().is_ok_and(|timestamp| timestamp > now)
            })
        },
    )
}

pub fn is_stale() -> impl Fn(&Request) -> bool + Copy {
    |request| request.session().is_none_or(Identity::is_expired)
}

pub fn unauthorized<T>() -> via::Result<T> {
    deny!(401, "unauthorized")
}

pub fn restore() -> RestoreSession {
    RestoreSession
}

pub async fn refresh(mut request: Request, next: Next) -> via::Result {
    let app = request.app_owned();

    if let Some(identity) = request
        .extensions_mut()
        .get_mut::<ProtectFromForgery>()
        .map(|session| session.identity_mut())
        && identity.user(app.database()).await.is_ok()
    {
        let mut identity = Some(identity.refresh());
        let mut response = next.call(request).await?;

        if response.status() == StatusCode::UNAUTHORIZED {
            identity = None;
        }

        response.authenticate(app.secret(), identity);

        Ok(response)
    } else {
        next.call(request).await
    }
}

#[inline(always)]
fn in_an_hour() -> i64 {
    (OffsetDateTime::now_utc() + Duration::hours(1)).unix_timestamp()
}

impl Identity {
    pub fn new(user: Id) -> Self {
        let mut buf = [0; TOKEN_LEN];

        buf[..EXPIRES_AT].copy_from_slice(user.to_bytes().as_slice());
        buf[EXPIRES_AT..].copy_from_slice(in_an_hour().to_be_bytes().as_slice());

        Self(buf)
    }

    pub fn expires_at(&self) -> Result<i64, Unauthorized> {
        self.0[EXPIRES_AT..]
            .try_into()
            .or(Err(Unauthorized))
            .map(i64::from_be_bytes)
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at()
            .is_ok_and(|timestamp| OffsetDateTime::now_utc().unix_timestamp() > timestamp)
    }

    pub async fn user(&self, database: &Database) -> Result<User, Error> {
        if let Some(user) = database.find_user(self.user_id()?).await? {
            Ok(user)
        } else {
            unauthorized()
        }
    }

    pub fn refresh(&mut self) -> Self {
        let new_expires_at = in_an_hour().to_be_bytes();

        self.0[EXPIRES_AT..].copy_from_slice(new_expires_at.as_slice());
        Self(self.0)
    }

    fn user_id(&self) -> Result<Id, Unauthorized> {
        let bytes = self.0[..EXPIRES_AT].try_into().or(Err(Unauthorized))?;
        Id::new(u64::from_be_bytes(bytes)).or(Err(Unauthorized))
    }
}

impl FromStr for Identity {
    type Err = via::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut buf = [0u8; TOKEN_LEN];

        if URL_SAFE_NO_PAD.decode_slice(input, &mut buf).is_err() {
            deny!(400, "unknown session cookie format.");
        }

        Ok(Identity(buf))
    }
}

impl ProtectFromForgery {
    fn identity(&self) -> &Identity {
        &self.0
    }

    fn identity_mut(&mut self) -> &mut Identity {
        &mut self.0
    }
}

impl Session for Request {
    fn session(&self) -> Option<&Identity> {
        self.extensions().get().map(ProtectFromForgery::identity)
    }

    async fn user(&self) -> Result<User, Error> {
        let session = self.session().ok_or(Unauthorized)?;
        let database = self.app().database();

        session.user(database).await
    }
}

#[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
impl Session for via::ws::Request<Unicorn> {
    fn session(&self) -> Option<&Identity> {
        self.extensions().get().map(ProtectFromForgery::identity)
    }

    async fn user(&self) -> Result<User, Error> {
        let session = self.session().ok_or(Unauthorized)?;
        let database = self.app().database();

        session.user(database).await
    }
}

impl Authenticate for Response {
    fn authenticate(&mut self, secret: &Key, identity: Option<Identity>) {
        // Build an empty session cookie.
        let mut cookie = Cookie::build(COOKIE)
            .http_only(true)
            .same_site(SameSite::Strict)
            .expires(OffsetDateTime::now_utc() + Duration::weeks(2))
            .secure(true)
            .path("/")
            .build();

        if let Some(Identity(value)) = identity {
            // Set the value of the cookie to the user.
            cookie.set_value(URL_SAFE_NO_PAD.encode(value.as_slice()));
        } else {
            // Indicates to the client that the cookie should be removed.
            cookie.make_removal();
        };

        // Add the session cookie.
        self.cookies_mut().signed_mut(secret).add(cookie);
    }
}

impl Middleware<Unicorn> for RestoreSession {
    fn call(&self, mut request: Request, next: Next) -> via::BoxFuture {
        let app = request.app();
        let jar = request.cookies().signed(app.secret());

        if let Some(cookie) = jar.get(COOKIE) {
            let session = match cookie.value().parse::<Identity>() {
                Ok(identity) => ProtectFromForgery(identity),
                Err(error) => return Box::pin(async { Err(error) }),
            };

            request.extensions_mut().insert(session);
        }

        next.call(request)
    }
}

impl From<Unauthorized> for via::Error {
    fn from(_: Unauthorized) -> Self {
        err!(401, "unauthorized.")
    }
}
