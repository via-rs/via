via::resource!(app = Unicorn);

use diesel::{BoolExpressionMethods, QueryDsl};
use serde::Serialize;
use via::request::PathParams;
use via::{Error, Response, ResultExt};
use via_diesel::paginate::{Keyset, LimitAndOffset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};

use crate::models::reaction::{self, top_reactions_for};
use crate::models::thread::{Thread, by_channel, by_id, recent, thread_id_is_null};
use crate::routes::channels::Subscriber;
use crate::schema::threads;
use crate::util::{Id, Iso8601};
use crate::{Next, Request, Unicorn};

/// List threads.
///
/// Responds to:
/// - `GET /api/channels/:channel-id/threads`
async fn index(request: Request, _: Next) -> via::Result {
    // Get the channel id from the subscription we loaded during authorization.
    // If the current user does not have an active subscription, 404 Not Found.
    let channel_id = request.channel_id().or_not_found()?;

    // Source keyset arguments from the URI query.
    // If the limit query param is > the default per page, 400 Bad Request.
    let by_keyset = request.query::<Keyset<Iso8601, Id, PER_PAGE>>()?;

    // Load a page of threads.
    let mut feed = {
        // Checkout a database connection.
        let mut connection = request.app().database().await?;

        // Load the threads in the channel, paginated `by_keyset`.
        let threads = Thread::query()
            .filter(by_channel(channel_id).and(thread_id_is_null()))
            .order(recent())
            .page(by_keyset.of(threads::created_at, threads::id))
            .load_async(&mut connection)
            .await?;

        // Side load the reactions to the threads in `threads`.
        let reactions = {
            let ids = Id::each(&threads).collect::<Vec<_>>();
            top_reactions_for(&mut connection, ids).await?
        };

        // Group the aggregated reactions with the thread to which they belong.
        reaction::group_by_thread(threads, reactions)
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

/// Find a thread by id.
///
/// Responds to `GET /api/channels/:channel-id/threads/:thread-id`.
async fn show(request: Request, _: Next) -> via::Result {
    // Get the channel id from the subscription we loaded during authorization.
    // If the current user does not have an active subscription, 404 Not Found.
    let channel_id = request.channel_id().or_not_found()?;

    // Parse a uuid from the :thread-id path parameter.
    let id = request.param("thread-id").parse()?;

    // Find the thread where id = :thread-id.
    let thread = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Find the matching thread.
        let thread = Thread::query()
            .filter(
                by_id(id)
                    .and(by_channel(channel_id))
                    .and(thread_id_is_null()),
            )
            .first_async(&mut connection)
            .await?;

        // Side load the reactions to `thread`.
        let reactions = top_reactions_for(&mut connection, vec![id]).await?;

        // Render the top reactions aggregation within `thread`.
        thread.with_reactions(reactions)
    };

    Response::build().data(thread)
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
