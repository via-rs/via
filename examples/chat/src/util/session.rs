use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use cookie::{Cookie, Key, SameSite};
use http::StatusCode;
use std::str::FromStr;
use time::{Duration, OffsetDateTime};
use via::error::{Catch, Error, Propagate};
use via::guard::{self, Predicate, on};
use via::{Middleware, Response, deny, err};

use crate::database::models::User;
use crate::database::{Database, Id};
use crate::{Request, Unicorn};

pub const COOKIE: &str = "via-chat-session";

const EXPIRES_AT: usize = 8;
const TOKEN_LEN: usize = 16;

pub trait Authenticate {
    fn authenticate(&mut self, secret: &Key, user: Option<Identity>);
}

pub trait Session {
    fn session(&self) -> Option<&Identity>;
    async fn user(&self) -> Result<User, Error>;
}

#[derive(Clone, Copy, PartialEq)]
pub struct Identity([u8; 16]);

#[derive(Clone, Copy)]
pub struct Unauthorized;

pub fn is_authenticated() -> impl for<'a> Predicate<Request, Error<'a> = &'a Unauthorized> {
    guard::ok_or(
        on::extension(guard::not(Identity::is_expired)),
        Unauthorized,
    )
}

pub fn needs_verified() -> impl for<'a> Predicate<Request, Error<'a> = ()> {
    guard::bool(on::extension(Identity::is_expired).opt())
}

pub fn unauthorized<T>() -> via::Result<T> {
    deny!(401, "unauthorized")
}

pub fn restore(request: &mut Request) -> Result<(), Catch> {
    let jar = {
        let secret = request.app().secret();
        request.cookies().signed(secret)
    };

    let token = jar
        .get(COOKIE)
        .ok_or(Unauthorized)
        .and_then(|cookie| cookie.value().parse::<Identity>())
        .or_continue()?;

    request.extensions_mut().insert(token);

    Ok(())
}

pub fn verify() -> impl Middleware<Unicorn> + 'static {
    via::middleware::<_, Unicorn>(|mut request, next| {
        let app = request.app_owned();

        Box::pin(async move {
            let identity = {
                let extensions = request.extensions_mut();
                let Some(token) = extensions.get_mut::<Identity>() else {
                    return next.call(request).await;
                };

                if let Ok(updated) = token.refresh(&app.database).await {
                    Some(updated)
                } else {
                    extensions.remove::<Identity>();
                    None
                }
            };

            let mut response = next.call(request).await?;

            if response.status() == StatusCode::UNAUTHORIZED {
                response.authenticate(&app.secret, None);
            } else {
                response.authenticate(&app.secret, identity);
            }

            Ok(response)
        })
    })
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

    pub async fn verify(&self, database: &Database) -> bool {
        if let Ok(id) = self.user_id() {
            database.user_exists(id).await
        } else {
            false
        }
    }

    pub async fn user(&self, database: &Database) -> Result<User, Error> {
        if let Some(user) = database.find_user(self.user_id()?).await? {
            Ok(user)
        } else {
            unauthorized()
        }
    }

    async fn refresh(&mut self, database: &Database) -> Result<Self, Unauthorized> {
        if database.user_exists(self.user_id()?).await {
            let new_expires_at = in_an_hour().to_be_bytes();
            self.0[EXPIRES_AT..].copy_from_slice(new_expires_at.as_slice());
            Ok(*self)
        } else {
            Err(Unauthorized)
        }
    }

    fn user_id(&self) -> Result<Id, Unauthorized> {
        let bytes = self.0[..EXPIRES_AT].try_into().or(Err(Unauthorized))?;
        Id::new(u64::from_be_bytes(bytes)).or(Err(Unauthorized))
    }

    fn encode(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.0.as_slice())
    }
}

impl FromStr for Identity {
    type Err = Unauthorized;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut buf = [0u8; TOKEN_LEN];

        URL_SAFE_NO_PAD
            .decode_slice(input, &mut buf)
            .map_or(Err(Unauthorized), |_| Ok(Identity(buf)))
    }
}

impl Session for Request {
    fn session(&self) -> Option<&Identity> {
        self.extensions().get()
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
        self.extensions().get()
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

        if let Some(token) = identity {
            cookie.set_value(token.encode());
        } else {
            cookie.make_removal();
        }

        // Add the session cookie.
        self.cookies_mut().signed_mut(secret).add(cookie);
    }
}

impl From<Unauthorized> for via::Error {
    fn from(_: Unauthorized) -> Self {
        err!(401, "unauthorized.")
    }
}

impl From<&'_ Unauthorized> for via::Error {
    fn from(error: &Unauthorized) -> Self {
        From::from(*error)
    }
}
