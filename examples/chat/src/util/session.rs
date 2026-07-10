use base64::engine::{Engine, general_purpose::URL_SAFE_NO_PAD};
use http::StatusCode;
use std::str::FromStr;
use time::{Duration, OffsetDateTime};
use via::error::{Catch, Propagate};
use via::guard::{self, Predicate, on};
use via::{Middleware, Response, err};

use super::id::Id;
use crate::app::Unicorn;
use crate::models::User;
use crate::{Next, Request};

const EXPIRES_AT: usize = 8;
const TOKEN_LEN: usize = 16;

#[derive(Clone, Copy, PartialEq)]
pub struct Identity([u8; 16]);

#[derive(Clone, Copy)]
pub struct Unauthorized;

pub trait Authentication {
    fn login(&self, user: User) -> via::Result;
    fn logout(&self, response: &mut Response);
    fn refresh(&self, identity: &Identity, response: &mut Response);
    fn restore(&self, request: &Request) -> Result<Identity, Unauthorized>;
}

pub trait Session {
    fn session(&self) -> Result<&Identity, Unauthorized>;
    fn me(&self) -> Result<Id, Unauthorized> {
        self.session().and_then(Identity::id)
    }
}

pub fn authenticate<T>(middleware: T) -> impl Middleware<Unicorn> + 'static
where
    T: Middleware<Unicorn> + 'static,
{
    guard::flat_map(auth_required(), middleware)
}

pub fn auth_required() -> impl for<'a> Predicate<Request, Error<'a> = &'a Unauthorized> {
    guard::ok_or(
        on::extension(guard::not(Identity::is_expired)),
        Unauthorized,
    )
}

pub fn needs_verified() -> impl for<'a> Predicate<Request, Error<'a> = ()> {
    guard::bool(on::extension(Identity::is_expired).opt())
}

pub fn restore(request: &mut Request) -> Result<(), Catch> {
    let identity = request.app().restore(request).or_continue()?;
    request.extensions_mut().insert(identity);
    Ok(())
}

pub fn verify() -> impl Middleware<Unicorn> + 'static {
    async |request: Request, next: Next| {
        let Some(id) = request.me().ok() else {
            return next.call(request).await;
        };

        // if let Ok(updated) = token.refresh(&app.database).await {
        //     Some(updated)
        // } else {
        //     extensions.remove::<Identity>();
        //     None
        // }

        let app = request.app_owned();
        let mut response = next.call(request).await?;

        if response.status() == StatusCode::UNAUTHORIZED {
            app.logout(&mut response);
        } else {
            let token = Identity::new(&id);
            app.refresh(&token, &mut response);
        }

        Ok(response)
    }
}

#[inline(always)]
fn in_an_hour() -> i64 {
    (OffsetDateTime::now_utc() + Duration::hours(1)).unix_timestamp()
}

impl Identity {
    pub fn new(id: &Id) -> Self {
        let mut buf = [0; TOKEN_LEN];

        buf[..EXPIRES_AT].copy_from_slice(id.value().to_be_bytes().as_slice());
        buf[EXPIRES_AT..].copy_from_slice(in_an_hour().to_be_bytes().as_slice());

        Self(buf)
    }

    pub fn id(&self) -> Result<Id, Unauthorized> {
        if let Ok(bytes) = self.0[..EXPIRES_AT].try_into() {
            Ok(Id::new(i64::from_be_bytes(bytes)))
        } else {
            Err(Unauthorized)
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at()
            .is_ok_and(|timestamp| OffsetDateTime::now_utc().unix_timestamp() > timestamp)
    }

    // #[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
    // pub async fn verify(&self, database: &Database) -> bool {
    //     if let Ok(id) = self.user_id() {
    //         database.user_exists(id).await
    //     } else {
    //         false
    //     }
    // }

    // pub async fn user(&self, database: &Database) -> Result<User, Error> {
    //     if let Some(user) = database.find_user(self.user_id()?).await? {
    //         Ok(user)
    //     } else {
    //         unauthorized()
    //     }
    // }

    // async fn refresh(&mut self, database: &Database) -> Result<Self, Unauthorized> {
    //     if database.user_exists(self.user_id()?).await {
    //         let new_expires_at = in_an_hour().to_be_bytes();
    //         self.0[EXPIRES_AT..].copy_from_slice(new_expires_at.as_slice());
    //         Ok(*self)
    //     } else {
    //         Err(Unauthorized)
    //     }
    // }

    pub fn encode(&self) -> String {
        URL_SAFE_NO_PAD.encode(&self.0)
    }

    fn expires_at(&self) -> Result<i64, Unauthorized> {
        if let Ok(bytes) = self.0[EXPIRES_AT..].try_into() {
            Ok(i64::from_be_bytes(bytes))
        } else {
            Err(Unauthorized)
        }
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
    fn session(&self) -> Result<&Identity, Unauthorized> {
        self.extensions().get().ok_or(Unauthorized)
    }

    // async fn user(&self) -> Result<User, Error> {
    //     let session = self.session().ok_or(Unauthorized)?;
    //     let database = self.app().database();

    //     session.user(database).await
    // }
}

#[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
impl Session for via::ws::Request<Unicorn> {
    fn session(&self) -> Result<&Identity, Unauthorized> {
        self.extensions().get().ok_or(Unauthorized)
    }

    // async fn user(&self) -> Result<User, Error> {
    //     let session = self.session().ok_or(Unauthorized)?;
    //     let database = self.app().database();

    //     session.user(database).await
    // }
}

impl From<Unauthorized> for via::Error {
    fn from(_: Unauthorized) -> Self {
        err!(401, "unauthorized")
    }
}

impl From<&'_ Unauthorized> for via::Error {
    fn from(error: &Unauthorized) -> Self {
        From::from(*error)
    }
}
