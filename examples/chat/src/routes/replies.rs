via::resource!(app = Unicorn);

use diesel::{BoolExpressionMethods, QueryDsl};
use serde::Serialize;
use via::request::PathParams;
use via::{Error, Response, ResultExt, deny};
use via_diesel::paginate::{Keyset, LimitAndOffset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};

use crate::models::thread::{by_channel, by_thread, recent};
use crate::models::{Reaction, Thread, ThreadWithUser};
use crate::routes::channels::Subscriber;
use crate::schema::threads;
use crate::util::{Id, Iso8601};
use crate::{Next, Request, Unicorn};

#[derive(Clone, Debug, Serialize)]
pub struct ReplyParams {
    thread_id: Id,
    reply_id: Id,
}

/// List replies to a thread.
///
/// Responds to:
/// - `GET /api/channels/:channel-id/threads/:thread-id/replies`
async fn index(request: Request, _: Next) -> via::Result {
    // Get the channel id from the subscription we loaded during authorization.
    // If the current user does not have an active subscription, 404 Not Found.
    let channel_id = request.channel_id().or_not_found()?;

    // Parse an `Id` from the :thread-id path parameter.
    let thread_id = request.param("thread-id").parse()?;

    // Source keyset arguments from the URI query.
    // If the limit query param is > the default per page, 400 Bad Request.
    let by_keyset = request.query::<Keyset<Iso8601, Id, PER_PAGE>>()?;

    // Load the replies to the thread with `thread_id`.
    let mut feed = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Load the replies to the thread, paginated by `keyset_args`.
        let threads = ThreadWithUser::query()
            .filter(by_channel(channel_id).and(by_thread(thread_id)))
            .order(recent())
            .page(by_keyset.of(threads::created_at, threads::id))
            .load_async(&mut connection)
            .await?;

        // Side load the reactions to the threads in `threads`.
        Reaction::to_threads(&mut connection, threads).await?
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
/// Responds to:
/// - `GET /api/channels/:channel-id/threads/:thread-id/replies/:reply-id`
async fn show(request: Request, _: Next) -> via::Result {
    // Parse an Id from the :reply-id path parameter.
    let id = request.param(":reply-id").parse()?;

    // Find the reply with an id = :reply-id.
    let reply = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Execute the query.
        Thread::find(&mut connection, id).await?
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

/// Delete a user account.
///
/// Responds to `DELETE /users/:user-id`.
///
/// The active user must be the user identified by `:user-id`.
async fn destroy(_: Request, _: Next) -> via::Result {
    todo!()
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
