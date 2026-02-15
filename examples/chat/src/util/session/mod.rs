mod identity;

use cookie::{Cookie, Key, SameSite};
use diesel::dsl::count;
use diesel::prelude::*;
use http::{Extensions, StatusCode};
use time::{Duration, OffsetDateTime};
use via::{Response, ws};

use self::identity::Identity;
use super::error;
use crate::chat::Chat;
use crate::models::user::*;
use crate::schema::users;
use crate::util::{DebugQueryDsl, Id};
use crate::{Next, Request};

/// Prevents the user type from being accessed in extensions outside this
/// module.
///
#[derive(Clone)]
struct Verify(Id);

pub const COOKIE: &str = "via-chat-session";

pub trait Authenticate {
    fn set_user(&mut self, secret: &Key, user: Option<Id>) -> via::Result<()>;
}

pub trait Session {
    fn user(&self) -> via::Result<&Id>;

    /// Returns an error if there is no user associated with the request.
    ///
    fn authenticate(&self) -> via::Result<()> {
        self.user().and(Ok(()))
    }
}

pub async fn restore(mut request: Request, next: Next) -> via::Result {
    let app = request.app_owned();
    let persist = match request
        .envelope()
        .cookies()
        .signed(request.app().secret())
        .get(COOKIE)
        .map(|cookie| cookie.value().parse::<Identity>())
    {
        Some(Ok(identity)) if identity.is_expired() => {
            if let ..1 = User::query()
                .select(count(users::id))
                .filter(by_id(identity.id()))
                .debug_result(&mut app.database().await?)
                .await?
            {
                return unauthorized(app.secret());
            }

            let session = Verify(*identity.id());
            request.envelope_mut().extensions_mut().insert(session);

            Some(identity.into())
        }
        Some(Ok(identity)) => {
            let session = Verify(identity.into());
            request.envelope_mut().extensions_mut().insert(session);

            None
        }
        Some(Err(error)) => {
            if cfg!(debug_assertions) {
                eprintln!("error: {}", error);
            }

            return unauthorized(app.secret());
        }
        None => None,
    };

    let mut response = next.call(request).await?;

    if let Some(id) = persist
        && response.status().is_success()
    {
        response.set_user(app.secret(), Some(id))?;
    }

    Ok(response)
}

fn unauthorized(secret: &Key) -> via::Result {
    let mut response = Response::build()
        .status(StatusCode::UNAUTHORIZED)
        .json(&error::unauthorized::<()>().err())?;

    response.set_user(secret, None)?;

    Ok(response)
}

fn identify(extensions: &Extensions) -> via::Result<&Id> {
    if let Some(Verify(id)) = extensions.get() {
        Ok(id)
    } else {
        error::unauthorized()
    }
}

impl Session for Request {
    fn user(&self) -> via::Result<&Id> {
        identify(self.extensions())
    }
}

impl Session for ws::Request<Chat> {
    fn user(&self) -> via::Result<&Id> {
        identify(self.envelope().extensions())
    }
}

impl Authenticate for Response {
    fn set_user(&mut self, secret: &Key, user: Option<Id>) -> via::Result<()> {
        // Build an empty session cookie.
        let mut session = Cookie::build(COOKIE)
            .http_only(true)
            .same_site(SameSite::Strict)
            .expires(OffsetDateTime::now_utc() + Duration::weeks(2))
            .secure(true)
            .path("/")
            .build();

        if let Some(id) = user {
            // Set the value of the cookie to the user.
            session.set_value(Identity::new(id).encode());
        } else {
            // Indicates to the client that the cookie should be removed.
            session.make_removal();
        };

        // Add the session cookie.
        self.cookies_mut().signed_mut(secret).add(session);

        Ok(())
    }
}
