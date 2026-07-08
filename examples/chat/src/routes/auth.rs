//! The /api/auth namespace.

use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::Deserialize;
use via::request::Payloadz;
use via::{Response, ResultExt, deny};
use zeroize::Zeroizing;

use crate::database::users;
use crate::models::user::{User, by_id, by_username};
use crate::util::session::{Authenticate, Identity, Session, Unauthorized};
use crate::{Next, Request};

#[derive(Deserialize)]
struct Login {
    username: String,
    password: Zeroizing<String>,
}

/// Authenticates the user identified by the credentials in the request body.
///
/// Responds to `POST /auth`.
///
/// If successful, the authenticated user will be returned in the response
/// and the session cookie will be set to the authenticated users identity.
///
/// If there is already a session associated with the request, a 403 Forbidden
/// response is returned.
pub async fn login(request: Request, _: Next) -> via::Result {
    // Deny the request if it comes from an authenticated user.
    if request.session().is_ok() {
        deny!(403, "session already exists");
    }

    // Prepare to read the request body.
    let (body, app) = request.into_future();

    // Find the user with the matching set of credentials.
    let user = {
        // Acquire a database connection and read the request body.
        let (mut conn, payload) = tokio::try_join!(
            app.acquire_database_connection(),
            body.timeout_after_secs(2)
        )?;

        // Deserialize login params from the request body.
        let params = payload.be_z_data::<Login>()?;
        //                   ^^^^^^^^^
        // Best-effort zeroization of the original request buffer containing the
        // unencrypted password.
        //
        // Prefer zeroizing payloads whenever they contain credentials.

        users::table
            .select(User::as_select())
            .filter(by_username(&params.username))
            .first(&mut conn)
            .await
            .optional()?
            .ok_or(Unauthorized)?
    };

    // Create an identity token for the active user.
    let identity = Identity::new(user.id());

    // Build a response containing the active user.
    let mut response = Response::build().status(201).data(user)?;

    // Set the session cookie.
    response.authenticate(app.signer(), Some(identity));

    Ok(response)
}

/// Ends the session associated with the request.
///
/// Responds to `DELETE /auth`.
///
/// If successful, an empty 204 No Content response is returned.
///
/// If there is not a session associated with the request, a 404 Not Found
/// response is returned instead.
pub async fn logout(request: Request, _: Next) -> via::Result {
    // If there is not an active session, pretend we don't exist.
    if request.session().is_err() {
        deny!(404, "not found");
    }

    // Build an empty response with a 204 status code.
    let mut response = Response::build().status(204).finish()?;

    // Remove the session cookie.
    response.authenticate(request.app().signer(), None);

    Ok(response)
}

/// Find the user associated with the request.
///
/// Responds to `GET /auth/me`.
///
/// If there is an session associated with the request, the user will be
/// returned in the response along with a fresh identity token expiry.
///
/// If there is not an active user session associated with the request, a 404
/// Not Found response is returned.
pub async fn me(request: Request, _: Next) -> via::Result {
    // Load the active user with id = session.id.
    let user = {
        // Acquire a database connection.
        let mut conn = request.app().acquire_database_connection().await?;

        // Get the active user id from the session.
        let id = request.session().map(Identity::id)?;

        users::table
            .select(User::as_select())
            .filter(by_id(&id))
            .first(&mut conn)
            .await
            .optional()?
            .or_not_found()?
    };

    // Create a fresh identity token for the active user.
    let identity = Identity::new(user.id());

    // Build a response containing the active user.
    let mut response = Response::build().data(user)?;

    // Update the session cookie expiry.
    response.authenticate(request.app().signer(), Some(identity));

    Ok(response)
}
