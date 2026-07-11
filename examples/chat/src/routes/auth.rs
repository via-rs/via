//! The /api/auth namespace.

use via::request::Payloadz;
use via::{Response, deny};
use via_diesel::prelude::*;

use crate::models::user::{User, by_id};
use crate::util::{Authentication, Identity, Session};
use crate::{Next, Request};

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
    if request.me().is_ok() {
        deny!(403, "session already exists");
    }

    // Read the request body and then deserialize AuthParams from the
    // "data" field of the JSON object in the request body.
    let (body, app) = request.into_future();
    let params = body.timeout_after_secs(2).await?.be_z_data()?;
    //                                             ^^^^^^^^^
    // Best-effort zeroization of the original request body containing the
    // plain text password prevents a secret from lingering in memory.

    // Find the user with the matching set of credentials.
    let user = {
        // Acquire a database connection.
        let mut connection = app.database().await?;

        // Authenticate the user.
        User::authenticate(&mut connection, params).await?
    };

    app.login(user)
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
    if request.me().is_err() {
        deny!(404, "not found");
    }

    // Build an empty response with a 204 status code.
    let mut response = Response::build().status(204).finish()?;

    // Remove the session cookie.
    request.app().logout(&mut response);

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
    // Get the active user id from the session.
    let id = request.me()?;

    // Find the active user with id = session.id.
    let user = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Execute the query.
        User::query()
            .filter(by_id(&id))
            .first(&mut connection)
            .await?
    };

    // Build a response containing the active user.
    let mut response = Response::build().data(user)?;

    // Update the the session cookie expiry and ttl.
    request.app().refresh(&Identity::new(&id), &mut response);

    Ok(response)
}
