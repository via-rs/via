//! The /api/users resource.
//!
//! `index` and all member actions require an authenticated user. `create` is
//! public so visitors can register an account.
//!
//! After an account is created, the response updates the session cookie to
//! authenticate the new user.

via::resource!(app = Unicorn, guard = [index, member]);

use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use via::request::Payloadz;
use via::{Payload, Response, ResultExt, deny};

use crate::database::{Id, users};
use crate::models::user::{ChangeSet, NewUser, User, by_id, recent};
use crate::util::{Authenticate, Identity, Session};
use crate::{Next, Request, Unicorn};

/// List users.
///
/// Responds to `GET /users`.
async fn index(request: Request, _: Next) -> via::Result {
    // Get pagination params from the URI query.
    // let page = request.query::<Page>()?;

    // Execute the query.
    let users = {
        // Acquire a database connection.
        let mut conn = request.app().acquire_database_connection().await?;

        users::table
            .select(User::as_select())
            .order(recent())
            .load(&mut conn)
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
    if request.session().is_err() {
        deny!(403, "logout before creating a new account");
    }

    // Prepare to read the request body.
    let (body, app) = request.into_future();

    // Insert the user into the users table.
    let user = {
        // Acquire a database connection and read the request body.
        let (mut conn, payload) = tokio::try_join!(
            app.acquire_database_connection(),
            body.timeout_after_secs(2)
        )?;

        // Deserialize a NewUser from the request body.
        let new_user = payload.be_z_data::<NewUser>()?;
        //                     ^^^^^^^^^
        // Best-effort zeroization of the original request buffer containing the
        // unencrypted password. The password type used by `NewUser` is also
        // zeroized on drop.
        //
        // Prefer zeroizing payloads whenever they contain credentials.

        diesel::insert_into(users::table)
            .values(new_user)
            .returning(User::as_returning())
            .get_result(&mut conn)
            .await?
    };

    // Create an identity token for the new user.
    let identity = Identity::new(user.id());

    // Build the response containing the newly inserted user.
    let mut response = Response::build().status(201).data(user)?;

    // Update the session cookie with the identity token of the new user.
    response.authenticate(app.signer(), Some(identity));

    Ok(response)
}

/// Retrieve a user by id.
///
/// Responds to `GET /users/:user-id`.
async fn show(request: Request, _: Next) -> via::Result {
    // Parse an Id from the :user-id path parameter.
    let id = request.param("user-id").parse::<Id>()?;

    // Find the user with id = :user-id.
    let user = {
        // Acquire a database connection.
        let mut conn = request.app().acquire_database_connection().await?;

        users::table
            .select(User::as_select())
            .filter(by_id(&id))
            .first(&mut conn)
            .await
            .optional()?
            .or_not_found()?
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
    if !request.session().is_ok_and(|session| id == session.id()) {
        deny!(403, "only the account owner can update a user");
    }

    // Prepare to read the request body.
    let (body, app) = request.into_future();

    // Apply the change set to the active user.
    let user = {
        // Acquire a database connection and read the request body.
        let (mut conn, payload) = tokio::try_join!(
            app.acquire_database_connection(),
            body.timeout_after_secs(2)
        )?;

        // Deserialize a user change set from the request body.
        let change_set = payload.data::<ChangeSet>()?;

        diesel::update(users::table)
            .filter(by_id(&id))
            .set(change_set)
            .returning(User::as_returning())
            .get_result(&mut conn)
            .await
            .optional()?
            .or_not_found()?
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
    if !request.session().is_ok_and(|session| id == session.id()) {
        deny!(403, "only the account owner can delete a user");
    } else {
        // Acquire a database connection.
        let mut conn = request.app().acquire_database_connection().await?;

        // Delete the user.
        diesel::delete(users::table)
            .filter(by_id(&id))
            .execute(&mut conn)
            .await
            .optional()?
            .or_not_found()?;
    }

    Response::build().status(204).finish()
}
