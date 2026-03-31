use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use cookie::{Cookie, Key, SameSite};
use std::str::FromStr;
use time::{Duration, OffsetDateTime};
use via::{Error, Response, raise};

use crate::database::Id;
use crate::database::models::User;
use crate::{Next, Request};

const ENCODED_LEN: usize = 24;
const EXPIRES_AT: usize = 8;
const TOKEN_LEN: usize = 16;

pub const COOKIE: &str = "via-chat-session";

pub trait Authenticate {
    fn authenticate(&mut self, secret: &Key, user: Option<Identity>);
}

pub trait Session {
    fn session(&self) -> via::Result<&Identity>;
    async fn user(&self) -> via::Result<User>;
}

#[derive(Clone, PartialEq)]
pub struct Identity([u8; 16]);

#[derive(Clone)]
struct ProtectFromForgery {
    identity: Identity,
}

pub async fn restore(mut request: Request, next: Next) -> via::Result {
    let Ok(mut refresh_token) = request
        .cookies()
        .signed(request.app().secret())
        .get(COOKIE)
        .map(|cookie| cookie.value().parse::<Identity>())
        .transpose()
    else {
        raise!(400, message = "unknown session cookie format.");
    };

    if let Some(mut identity) = refresh_token {
        refresh_token = if identity.is_expired() {
            let Some(_) = request
                .app()
                .database()
                .find_user(identity.user_id()?)
                .await?
            else {
                raise!(401, message = "unauthorized.");
            };

            identity.refresh();

            request
                .extensions_mut()
                .insert(ProtectFromForgery { identity })
                .map(|session| session.identity)
        } else {
            request
                .extensions_mut()
                .insert(ProtectFromForgery { identity })
                .and(None)
        }
    }

    let app = request.app_owned();
    let mut response = next.call(request).await?;

    match response.status().as_u16() {
        200..=399 if refresh_token.is_some() => response.authenticate(app.secret(), refresh_token),
        401 => response.authenticate(app.secret(), None),
        _ => {}
    }

    Ok(response)
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

    pub fn user_id(&self) -> Result<Id, Error> {
        let Ok(bytes) = self.0[..EXPIRES_AT].try_into() else {
            raise!(400, message = "unknown session cookie format.");
        };

        let Ok(id) = Id::new(u64::from_be_bytes(bytes)) else {
            raise!(401, message = "unauthorized.");
        };

        Ok(id)
    }

    fn is_expired(&self) -> bool {
        self.0[EXPIRES_AT..].try_into().is_ok_and(|bytes| {
            OffsetDateTime::now_utc().unix_timestamp() > i64::from_be_bytes(bytes)
        })
    }

    fn refresh(&mut self) {
        let new_expires_at = in_an_hour().to_be_bytes();
        self.0[EXPIRES_AT..].copy_from_slice(new_expires_at.as_slice());
    }
}

impl FromStr for Identity {
    type Err = via::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut buf = [0u8; TOKEN_LEN];

        if input.len() != ENCODED_LEN || URL_SAFE_NO_PAD.decode_slice(input, &mut buf).is_err() {
            raise!(400, message = "unknown session cookie format.");
        }

        Ok(Identity(buf))
    }
}

impl Session for Request {
    fn session(&self) -> via::Result<&Identity> {
        let Some(session) = self.extensions().get::<ProtectFromForgery>() else {
            raise!(401);
        };

        Ok(&session.identity)
    }

    async fn user(&self) -> via::Result<User> {
        let id = self.session().and_then(Identity::user_id)?;
        let Some(user) = self.app().database().find_user(id).await? else {
            raise!(401);
        };

        Ok(user)
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
            cookie.set_value(URL_SAFE_NO_PAD.encode(value));
        } else {
            // Indicates to the client that the cookie should be removed.
            cookie.make_removal();
        };

        // Add the session cookie.
        self.cookies_mut().signed_mut(secret).add(cookie);
    }
}
