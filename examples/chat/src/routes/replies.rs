via::resource!(app = Unicorn);

use diesel::{BoolExpressionMethods, QueryDsl};
use http::StatusCode;
use serde::Serialize;
use via::request::PathParams;
use via::{Error, Response, ResultExt, deny};
use via_diesel::paginate::{Keyset, LimitAndOffset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};

use crate::models::reaction::{self, top_reactions_for};
use crate::models::thread::{Thread, by_channel, by_id, by_thread, by_user, recent};
use crate::routes::channels::Subscriber;
use crate::schema::threads;
use crate::util::{Id, Iso8601, Session};
use crate::{Next, Request, Unicorn};

#[derive(Clone, Debug, Serialize)]
pub struct ReplyParams {
    thread_id: Id,
    reply_id: Id,
}

/// List replies to a thread.
///
/// Responds to `GET /api/channels/:channel-id/threads/:thread-id/replies`.
async fn index(request: Request, _: Next) -> via::Result {
    // Get the channel id from the subscription we loaded during authorization.
    // If the current user does not have an active subscription, 404 Not Found.
    let channel_id = request.channel_id().or_not_found()?;

    // Parse a uuid from the :thread-id path parameter.
    let thread_id = request.param("thread-id").parse()?;

    // Source keyset arguments from the URI query.
    // If the limit query param is > the default per page, 400 Bad Request.
    let by_keyset = request.query::<Keyset<Iso8601, Id, PER_PAGE>>()?;

    // Load a page of replies where thread_id = :thread-id.
    let mut feed = {
        // Checkout a database connection.
        let mut connection = request.app().database().await?;

        // Load the threads in the channel, paginated `by_keyset`.
        let replies = Thread::query()
            .filter(by_channel(channel_id).and(by_thread(thread_id)))
            .order(recent())
            .page(by_keyset.of(threads::created_at, threads::id))
            .load_async(&mut connection)
            .await?;

        // Side load the reactions to the threads in `replies`.
        let reactions = {
            let ids = Id::each(&replies).collect::<Vec<_>>();
            top_reactions_for(&mut connection, ids).await?
        };

        // Group the aggregated reactions with the thread to which they belong.
        reaction::group_by_thread(replies, reactions)
    };

    feed.reverse(); // Presented as append-only.

    Response::build().data(feed)
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
/// Responds to `GET /api/channels/:channel-id/threads/:thread-id/replies/:reply-id`.
async fn show(request: Request, _: Next) -> via::Result {
    // Get the channel id from the subscription we loaded during authorization.
    // If the current user does not have an active subscription, 404 Not Found.
    let channel_id = request.channel_id().or_not_found()?;

    // Parse `ReplyParams` from the URI path parameters.
    let params = request.params::<ReplyParams>()?;

    // Find the reply where id = :reply-id.
    let reply = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Find the matching reply.
        let reply = Thread::query()
            .filter(
                by_id(params.reply_id)
                    .and(by_channel(channel_id))
                    .and(by_thread(params.thread_id)),
            )
            .first_async(&mut connection)
            .await?;

        // Side load the reactions to `reply`.
        let reactions = {
            let ids = vec![params.reply_id];
            top_reactions_for(&mut connection, ids).await?
        };

        // Render the top reactions aggregation within `reply`.
        reply.with_reactions(reactions)
    };

    Response::build().data(reply)
}

/// Update an existing user.
///
/// Responds to `PATCH /users/:user-id`.
///
/// The active user must be the user identified by `:user-id`.
async fn update(_: Request, _: Next) -> via::Result {
    todo!()
}

/// Delete a reply by id.
///
/// Responds to `DELETE /api/channels/:channel-id/threads/:thread-id/replies/:reply-id`.
///
/// The current user must be the user identified by `thread.user_id`.
async fn destroy(request: Request, _: Next) -> via::Result {
    // Get the current user's id from the session.
    let me = request.me()?;

    // Parse a uuid from the :thread-id path parameter.
    let params = request.params::<ReplyParams>()?;

    // Acquire a database connection.
    let mut connection = request.app().database().await?;

    // Execute the DELETE.
    // If the number of affected rows is < 1, 404 Not Found.
    if let ..1 = diesel::delete(threads::table)
        .filter(by_id(params.reply_id).and(by_user(me)))
        .execute_async(&mut connection)
        .await?
    {
        deny!(404, "not found");
    }

    Response::build().status(StatusCode::NO_CONTENT).finish()
}

impl<'a> TryFrom<PathParams<'a>> for ReplyParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            thread_id: params.get("thread-id").parse()?,
            reply_id: params.get("reply-id").parse()?,
        })
    }
}
