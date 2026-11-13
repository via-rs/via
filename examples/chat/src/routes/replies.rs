via::resource!(app = Unicorn);

use diesel::{BoolExpressionMethods, QueryDsl};
use serde::Serialize;
use via::request::PathParams;
use via::{Error, Response, ResultExt, deny};
use via_diesel::paginate::{Keyset, LimitAndOffset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};

use super::channels::Subscriber;
use super::threads::{page_by_keyset, update_by_id};
use crate::models::reaction::Reaction;
use crate::models::thread::{self, Thread, ThreadWithUser, UpdateThreadScope};
use crate::util::{Id, Iso8601, Session};
use crate::{Next, Request, Unicorn, schema::threads};

#[derive(Clone, Debug, Serialize)]
pub struct ReplyParams {
    thread_id: Id,
    reply_id: Id,
}

/// List replies to a thread.
///
/// Responds to:
/// - `GET /api/channels/:channel-id/threads/:thread-id/replies`
fn index(request: Request, _: Next) -> impl Future<Output = via::Result> {
    page_by_keyset(request)
}

/// Create a new thread or reply to a thread.
///
/// Responds to:
/// - `POST /api/channels/:channel-id/threads`
/// - `POST /api/channels/:channel-id/threads/:thread-id/replies`
async fn create(_: Request, _: Next) -> via::Result {
    todo!()
}

/// Retrieve a reply by id.
///
/// Responds to:
/// - `GET /api/channels/:channel-id/threads/:thread-id/replies/:reply-id`
async fn show(request: Request, _: Next) -> via::Result {
    // Parse an Id from the :reply-id path parameter.
    let id = request.param(":reply-id").parse()?;

    // Find the reply with an id = :reply-id.
    let reply = {
        // Acquire a database connection.
        let mut connection = request.app().database().get().await?;

        // Execute the query.
        Thread::find(&mut connection, id).await?
    };

    Response::build().data(reply)
}

/// Update an existing reply by id.
async fn update(request: Request, _: Next) -> via::Result {
    let scope = UpdateThreadScope::reply(
        // Get the active user id from the session.
        request.me()?,
        // Extract ReplyParams from the request uri path.
        request.params()?,
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
