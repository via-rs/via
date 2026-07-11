//! The /api/users resource.
//!
//! `index` and all member actions require an authenticated user. `create` is
//! public so visitors can register an account.
//!
//! After an account is created, the response updates the session cookie to
//! authenticate the new user.

via::resource!(app = Unicorn, guard = [index, member]);

use via::request::Payloadz;
use via::{Payload, Response, deny};
use via_diesel::{LimitAndOffset, prelude::*};

use crate::models::user::{User, by_id, recent};
use crate::util::{Authenticator, Id, Session};
use crate::{Next, Request, Unicorn};

/// List users.
///
/// Responds to `GET /users`.
async fn index(request: Request, _: Next) -> via::Result {
    // Get pagination params from the URI query.
    let limit_and_offset = request.query::<LimitAndOffset>()?;

    // Load a page of users.
    let users = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        User::query()
            .order(recent())
            .page(limit_and_offset)
            .load(&mut connection)
            .await?
    };

    Response::build().data(users)
}

/// Create a new user account.
///
/// Responds to `POST /users`.
///
/// On success, the session cookie is updated to authenticate the new account.
async fn create(request: Request, _: Next) -> via::Result {
    // Deny the request if it comes from an authenticated user.
    if request.me().is_ok() {
        deny!(403, "logout before creating a new account");
    }

    // Deserialize a NewUser from the request body.
    let (body, app) = request.into_future();
    let new_user = body.timeout_after_secs(2).await?.be_z_data()?;
    //                                               ^^^^^^^^^
    // Best-effort zeroization of the original request body containing the
    // plaintext password prevents the secret from lingering in memory.

    // Insert the user into the users table.
    let user = {
        // Acquire a database connection.
        let mut connection = app.database().await?;

        // Perform the insert.
        User::create(&mut connection, new_user).await?
    };

    app.login(user)
}

/// Retrieve a user by id.
///
/// Responds to `GET /users/:user-id`.
async fn show(request: Request, _: Next) -> via::Result {
    // Parse an Id from the :user-id path parameter.
    let id = request.param("user-id").parse::<Id>()?;

    // Find the user with id = `:user-id`.
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

/// Update an existing user.
///
/// Responds to `PATCH /users/:user-id`.
///
/// The active user must be the user identified by `:user-id`.
async fn update(request: Request, _: Next) -> via::Result {
    // Parse an Id from the :user-id path parameter.
    let id = request.param("user-id").parse::<Id>()?;

    // Confirm that the active user is the owner of the account.
    if !request.me().is_ok_and(|me| id == me) {
        deny!(403, "only the account owner can update a user");
    }

    // Aggregate the request body and deserialize a user change set.
    let (body, app) = request.into_future();
    let changes = body.timeout_after_secs(2).await?.data()?;

    // Apply the change set to the active user.
    let user = {
        // Acquire a database connection.
        let mut connection = app.database().await?;

        // Perform the update.
        User::update(&mut connection, id, changes).await?
    };

    Response::build().data(user)
}

/// Delete a user account.
///
/// Responds to `DELETE /users/:user-id`.
///
/// The active user must be the user identified by `:user-id`.
async fn destroy(request: Request, _: Next) -> via::Result {
    // Parse an Id from the :user-id path parameter.
    let id = request.param("user-id").parse::<Id>()?;

    // Confirm that the active user is the owner of the account.
    if !request.me().is_ok_and(|me| id == me) {
        deny!(403, "only the account owner can delete a user");
    } else {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Perform the delete.
        User::destroy(&mut connection, id).await?;
    }

    Response::build().status(204).finish()
}
