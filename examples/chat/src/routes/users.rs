//! The /api/users resource.
//!
//! `index` and all member actions require an authenticated user. `create` is
//! public so visitors can register an account.
//!
//! After an account is created, the response updates the session cookie to
//! authenticate the new user.

via::resource!(app = Unicorn, guard = member, only = [create, member]);

use diesel::{ExpressionMethods, Identifiable, QueryDsl};
use http::StatusCode;
use via::request::Payloadz;
use via::{Payload, Response, ResultExt, deny};
use via_diesel::paginate::{LimitAndOffset, LimitAndPage};
use via_diesel::{AsyncQueryDsl, Paginate};

use crate::models::user::{self, User, by_id, by_username_asc};
use crate::schema::users;
use crate::util::{Authenticator, Id, Session, hurry};
use crate::{Next, Request, Unicorn};

/// Create a new user.
async fn create(request: Request, _: Next) -> via::Result {
    // Deny the request if it comes from an authenticated user.
    if request.me().is_ok() {
        via::deny!(403, "you must first logout before creating a new account");
    }

    // Deserialize a NewUser from the request body.
    let (body, app) = request.into_future();
    let new_user = body.timeout_after_secs(2).await?.be_z_data()?;
    //                                               ^^^^^^^^^
    // Best-effort zeroization of the original request body containing the
    // plaintext password prevents the secret from lingering in memory.

    // Insert the user into the users table.
    let user = hurry!(async {
        let mut connection = app.database().get().await?;
        User::create(&mut connection, new_user).await
    })?;

    // Refresh the session and generate a response.
    app.login(&user)
}

/// Find a user by id.
async fn show(request: Request, _: Next) -> via::Result {
    // Parse an Id from the :user-id path parameter.
    let id = request.param("user-id").parse::<Id>()?;

    // Find the user with id = `:user-id`.
    let user = hurry!(async {
        let mut connection = request.app().database().get().await?;

        User::query()
            .filter(users::id.eq(&id))
            .first_async(&mut connection)
            .await
    })?;

    Response::build().data(&user)
}

/// Update a user by id.
async fn update(request: Request, _: Next) -> via::Result {
    // Confirm that the request target is owned by the current user.
    let me = hey_its_me(&request)?;

    // Aggregate the request body and deserialize a user change set.
    let (body, app) = request.into_future();
    let changes = body.timeout_after_secs(2).await?.data()?;

    // Apply the change set to the current user.
    let user = hurry!(async {
        let mut connection = app.database().get().await?;
        User::update(&mut connection, &me, changes).await
    })?;

    Response::build().data(&user)
}

/// Delete a user by id.
async fn destroy(request: Request, _: Next) -> via::Result {
    // Confirm that the request target is owned by the current user.
    let me = hey_its_me(&request)?;

    // Checkout a database connection and then delete the user.
    let _: Id = hurry!(async {
        let mut connection = request.app().database().get().await?;

        diesel::delete(users::table)
            .filter(users::id.eq(&me))
            .returning(users::id)
            .get_result_async(&mut connection)
            .await
    })?;

    Response::build().status(StatusCode::NO_CONTENT).finish()
}

/// Confirm that the request target is the currently authenticated user.
/// Returns the id of the current user if the `:user-id` path param is a match.
fn hey_its_me(request: &Request) -> via::Result<Id> {
    // Get the id of the current user from the session.
    let me = request.me()?;

    // Parse an Id from the :user-id path parameter.
    let id = request.param("user-id").parse::<Id>()?;

    // Deny the request if the current user's id is not :user-id.
    (me == id).then_some(me).or_forbidden()
}
