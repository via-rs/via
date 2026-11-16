use diesel::{BoolExpressionMethods, QueryDsl};
use http::StatusCode;
use serde::Serialize;
use via::{Error, Payload, Response, ResultExt};
use via_diesel::paginate::{Keyset, LimitAndOffset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};

use crate::models::thread::{self, UpdateThreadScope};
use crate::models::{Reaction, Thread, ThreadWithUser};
use crate::routes::channels::Subscriber;
use crate::util::{Id, Iso8601, hurry};
use crate::{Next, Request, Unicorn, schema::threads};

pub async fn page_by_keyset(request: Request) -> via::Result {
    // Get the channel id from the subscription we loaded during authorization.
    // If the current user does not have an active subscription, 404 Not Found.
    let channel_id = request.channel_id().or_not_found()?;

    // Parse an `Option<Id>` from the :thread-id path parameter.
    //
    // If this function is called from the replies resource, the `feed` will
    // contain the replies to :thread-id for the provided keyset.
    let parent_id = request.param("thread-id").ok_and_then(|id| id.parse())?;

    // Source keyset arguments from the URI query.
    // If the limit query param is > the default per page, 400 Bad Request.
    let by_keyset = request
        .query::<Keyset<Iso8601, Id, PER_PAGE>>()?
        .of(threads::created_at, threads::id);

    // Load a page of threads.
    let mut feed = {
        // Checkout a database connection.
        let mut connection = request.app().database().get().await?;

        // Apply the order and keyset to the ThreadWithUser query.
        //
        // Paginating by keyset require boxing the query and we have to branch
        // on `parent_id` to support the replies resource.
        let mut query = ThreadWithUser::query()
            .order(thread::recent())
            .page(by_keyset);

        if let Some(thread_id) = parent_id.as_ref() {
            query = query.filter(thread::by_channel(channel_id).and(thread::by_thread(*thread_id)));
        } else {
            query = query.filter(thread::by_channel(channel_id).and(thread::is_thread()));
        }

        // Load the threads in the channel, paginated `by_keyset`.
        let threads = query.load_async(&mut connection).await?;

        // Side load the reactions to the threads in `threads`.
        Reaction::to_threads(&mut connection, threads).await?
    };

    feed.reverse(); // Presented as append-only.

    Response::build().data(feed)
}

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
