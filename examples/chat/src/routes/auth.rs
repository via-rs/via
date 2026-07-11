//! The /api/auth namespace.

use http::StatusCode;
use via::request::Payloadz;
use via::{Response, deny, err};
use via_diesel::prelude::*;

use crate::models::user::{User, by_id};
use crate::util::{Authenticator, Session};
use crate::{Next, Request};

/// Authenticates the user identified by the credentials in the request body.
///
/// Responds to `POST /api/auth`.
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
/// Responds to `DELETE /api/auth`.
///
/// If successful, an empty 204 No Content response is returned.
///
/// If there is not a session associated with the request, 403 Forbidden.
pub async fn logout(request: Request, _: Next) -> via::Result {
    // If there is not an active session, pretend we don't exist.
    let mut response = if request.me().is_ok() {
        // Build an empty response with a 204 status code.
        Response::build().status(StatusCode::NO_CONTENT).finish()?
    } else {
        // Build an eager error response so we can destroy the session.
        Response::build()
            .status(StatusCode::FORBIDDEN)
            .errors(err!(403, "forbidden"))?
    };

    // Instruct the client to remove the session cookie.
    request.app().logout(&mut response);

    Ok(response)
}

/// Find the user associated with the request.
///
/// Responds to `GET /api/auth/me`.
///
/// If there is an session associated with the request, the user will be
/// returned in the response along with a fresh identity token expiry.
///
/// If there is not an active user session associated with the request, a 404
/// Not Found response is returned.
pub async fn me(request: Request, _: Next) -> via::Result {
    // Get the active user id from the session.
    let id = request.me()?;

    // Find the active user.
    let user = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Execute the query.
        User::query()
            .filter(by_id(&id))
            .first(&mut connection)
            .await?
    };

    Response::build().data(user)
}
