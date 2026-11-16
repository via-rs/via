via::resource!(app = Unicorn);

mod helper;

pub(super) use helper::{page_by_keyset, update_by_id};

use diesel::{BoolExpressionMethods, QueryDsl};
use http::StatusCode;
use serde::Serialize;
use via::request::PathParams;
use via::{Error, Payload, Response, ResultExt};
use via_diesel::paginate::{Keyset, LimitAndOffset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};

use crate::models::Reaction;
use crate::models::thread::{self, ChangeSet, Thread, ThreadWithUser, UpdateThreadScope};
use crate::routes::channels::Subscriber;
use crate::schema::threads;
use crate::util::{Id, Iso8601, Session, hurry};
use crate::{Next, Request, Unicorn};

/// List threads.
///
/// Responds to:
/// - `GET /api/channels/:channel-id/threads`
fn index(request: Request, _: Next) -> impl Future<Output = via::Result> {
    page_by_keyset(request)
}

/// Create a new thread or reply to a thread.
async fn create(_: Request, _: Next) -> via::Result {
    todo!()
}

/// Retrieve a thread or reply by id.
///
/// Responds to:
/// - `GET /api/channels/:channel-id/threads/:thread-id`
async fn show(request: Request, _: Next) -> via::Result {
    // Parse an Id from the :thread-id path parameter.
    let id = request.param("thread-id").parse()?;

    // Checkout a database connection and find the thread with id.
    let thread = hurry!(async {
        let mut connection = request.app().database().get().await?;
        Thread::find(&mut connection, id).await
    })?;

    Response::build().data(thread)
}

/// Update an existing thread by id.
async fn update(request: Request, _: Next) -> via::Result {
    let scope = UpdateThreadScope::thread(
        // Get the active user id from the session.
        request.me()?,
        // Extract the thread id from the request uri path.
        request.param("thread-id").parse()?,
    );

    update_by_id(request, &scope).await
}

/// Delete a user account.
///
/// Responds to `DELETE /users/:user-id`.
///
/// The active user must be the user identified by `:user-id`.
async fn destroy(_: Request, _: Next) -> via::Result {
    todo!()
}
