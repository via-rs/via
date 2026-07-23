use base64::engine::{Engine, GeneralPurpose};
use std::str::FromStr;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;
use via::guard::bytes::case_sensitive;
use via::guard::{self, Predicate, method, on};
use via::{Middleware, Response, err};

use super::id::Id;
use crate::Request;
use crate::app::{IdentityExtension, Unicorn};
use crate::models::User;

const EXPIRES_AT: usize = 16;
const TOKEN_LEN: usize = 24;

/// The codec used to decode or encode an identity token.
pub const CODEC: GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

#[derive(Clone, PartialEq)]
pub struct Identity([u8; TOKEN_LEN]);

pub struct Unauthorized;

pub trait Authenticator {
    fn login(&self, user: User) -> via::Result;
    fn logout(&self, response: &mut Response);
}

pub trait Session {
    fn me(&self) -> Result<Id, Unauthorized>;
}

pub fn authenticate<T>(middleware: T) -> impl Middleware<Unicorn> + 'static
where
    T: Middleware<Unicorn> + 'static,
{
    guard::flat_map(auth_required(), middleware)
}

pub fn auth_required() -> impl for<'a> Predicate<Request, Error<'a> = &'a Unauthorized> {
    guard::ok_or(
        on(guard::not(Identity::is_expired), IdentityExtension),
        Unauthorized,
    )
}

pub fn needs_verified() -> impl for<'a> Predicate<Request, Error<'a> = ()> {
    let is_auth_request = on::path(case_sensitive(b"/api/auth"));

    guard::or((
        // The request is a mutation and the target is not /api/auth.
        (method::is_mutation(), guard::not(is_auth_request)),
        // The active user account has not been verified in the past hour.
        guard::bool(on(Identity::is_expired, IdentityExtension).opt()),
    ))
}

#[inline(always)]
fn in_an_hour() -> i64 {
    (OffsetDateTime::now_utc() + Duration::hours(1)).unix_timestamp()
}

impl Identity {
    pub fn new(id: Id) -> Self {
        let mut buf = [0; TOKEN_LEN];

        buf[..EXPIRES_AT].copy_from_slice(id.value().as_ref());
        buf[EXPIRES_AT..].copy_from_slice(in_an_hour().to_be_bytes().as_slice());

        Self(buf)
    }

    pub fn id(&self) -> Result<Id, Unauthorized> {
        if let Ok(bytes) = self.0[..EXPIRES_AT].try_into() {
            Ok(Id::new(Uuid::from_bytes(bytes)))
        } else {
            Err(Unauthorized)
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at()
            .is_ok_and(|timestamp| OffsetDateTime::now_utc().unix_timestamp() > timestamp)
    }

    fn expires_at(&self) -> Result<i64, Unauthorized> {
        if let Ok(bytes) = self.0[EXPIRES_AT..].try_into() {
            Ok(i64::from_be_bytes(bytes))
        } else {
            Err(Unauthorized)
        }
    }
}

impl AsRef<[u8]> for Identity {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl FromStr for Identity {
    type Err = Unauthorized;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut buf = [0u8; TOKEN_LEN];

        if CODEC.decode_slice(input, &mut buf).is_ok() {
            Ok(Self(buf))
        } else {
            Err(Unauthorized)
        }
    }
}

impl From<Unauthorized> for via::Error {
    fn from(_: Unauthorized) -> Self {
        err!(401, "unauthorized")
    }
}

impl From<&'_ Unauthorized> for via::Error {
    fn from(_: &Unauthorized) -> Self {
        err!(401, "unauthorized")
    }
}
