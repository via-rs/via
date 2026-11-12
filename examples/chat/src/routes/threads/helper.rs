use diesel::{BoolExpressionMethods, QueryDsl};
use http::StatusCode;
use serde::Serialize;
use via::{Error, Payload, Response, ResultExt};
use via_diesel::paginate::{Keyset, LimitAndOffset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};

use crate::models::thread::{Thread, ThreadWithUser, UpdateThreadScope};
use crate::util::{Id, hurry};
use crate::{Next, Request, Unicorn};

/// Update an existing thread with the provided update scope.
///
/// This function abstracts the update business logic for threads and replies
/// into a single function without putting controller concerns in the Thread
/// model.
///
/// The current user must be authorized to access the channel to which the
/// thread belongs and own the thread with `id = scope.target_id`. Otherwise,
/// `404`.
pub async fn update_by_id(request: Request, scope: &UpdateThreadScope) -> via::Result {
    // Aggregate the request body and deserialize a thread change set.
    let (body, app) = request.into_future();
    let changes = body.timeout_after_secs(2).await?.data()?;

    // Checkout a database connection and update the thread.
    let thread = hurry!(async {
        let mut connection = app.database().get().await?;
        Thread::update(&mut connection, scope, changes).await
    })?;

    // Respond with JSON if the thread was modified. Otherwise, return an empty
    // response with a 204 status code.
    if thread.did_change() {
        Response::build().json(&thread)
    } else {
        Response::build().status(StatusCode::NO_CONTENT).finish()
    }
}
