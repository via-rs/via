via::resource!(app = Unicorn);

use diesel::{BoolExpressionMethods, BoxableExpression, ExpressionMethods, QueryDsl};
use http::StatusCode;
use serde::Serialize;
use via::{Error, Payload, Response, ResultExt};
use via_diesel::paginate::{Keyset, LimitAndOffset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};
use via_pubsub::{Dispatch, OurEvent};

use super::channels::Subscriber;
use super::chat::Relay;
use crate::models::thread::{PutThread, ThreadCollectionContext, ThreadMemberContext, recent};
use crate::models::{Reaction, Subscription, Thread, ThreadDetails, ThreadWithUser};
use crate::util::{Id, Iso8601, Session, hurry};
use crate::{Next, Request, Unicorn, schema::threads};

async fn index(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let context = ThreadCollectionContext::try_from(&request)?;

    // Source keyset arguments from the URI query.
    // If the limit query param is > the default per page, 400 Bad Request.
    let by_keyset = request
        .query::<Keyset<Iso8601, Id, PER_PAGE>>()?
        .of(threads::created_at, threads::id);

    // Preemptively free up stack space. This route requires multiple round
    // trips to the database.
    let (_, _, app) = request.into_parts();

    // Load a page of threads.
    let mut feed = {
        // Checkout a database connection.
        let mut connection = app.database().get().await?;

        // Apply the order and keyset to the ThreadWithUser query.
        //
        // Paginating by keyset require boxing the query and we have to branch
        // on `parent_id` to support the replies resource.
        let mut query = ThreadWithUser::query().order(recent()).page(by_keyset);

        if let Some(thread_id) = context.thread_id() {
            let expr = threads::channel_id
                .eq(context.channel_id())
                .and(threads::thread_id.eq(thread_id));

            query = query.filter(expr);
        } else {
            let expr = threads::channel_id
                .eq(context.channel_id())
                .and(threads::thread_id.is_null());

            query = query.filter(expr);
        }

        // Load the threads in the channel, paginated `by_keyset`.
        let threads = query.load_async(&mut connection).await?;

        // Side load the reactions to the threads in `threads`.
        Reaction::to_threads(&mut connection, threads).await?
    };

    feed.reverse(); // Presented as append-only.

    Response::build().data(&feed)
}

async fn create(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let context = ThreadCollectionContext::try_from(&request)?;

    // Deserialize a NewThread from the request body.
    let (body, app) = request.into_future();
    let new_thread = body.timeout_after_secs(2).await?.data::<PutThread>()?;

    // Checkout a database connection and perform the insert.
    let thread = hurry!(async {
        let mut connection = app.database().get().await?;
        Thread::create(&mut connection, &context, new_thread).await
    })?;

    // Generate the response with a reference to `thread` so we can move the
    // original value into the relay event.
    let response = Response::build()
        .status(StatusCode::CREATED)
        .data(&thread)?;

    // Dispatch the "new-reply" event to the subscribers in the channel.
    app.pubsub().dispatch(OurEvent::new_reply(thread));

    Ok(response)
}

async fn show(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let context = ThreadMemberContext::try_from(&request)?;

    // Request is no longer required once `ThreadMemberContext` is extracted
    // and this route requires making more than one database query over the
    // same connection. Preemptively free up stack space by taking app out
    // of request.
    let (_, _, app) = request.into_parts();

    // Prefer branching before checking out a database connection.
    let thread = if let Some(reply_id) = context.reply_id() {
        // Find an individual reply to a thread.
        let mut connection = app.database().get().await?;
        ThreadDetails::find_reply(&mut connection, context.thread_id(), reply_id).await?
    } else {
        // Find an individual reply to a thread.
        let mut connection = app.database().get().await?;
        ThreadDetails::find_thread(&mut connection, context.thread_id()).await?
    };

    Response::build().data(&thread)
}

/// Update an existing thread.
///
/// The current user must be authorized to access the channel to which the
/// thread belongs and own the thread. Otherwise, `404`.
async fn update(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let context = ThreadMemberContext::try_from(&request)?;

    // Aggregate the request body and deserialize a thread change set.
    let (body, app) = request.into_future();
    let change_set = body.timeout_after_secs(2).await?.data()?;

    // Checkout a database connection and update the thread.
    let thread = hurry!(async {
        let mut connection = app.database().get().await?;
        Thread::update(&mut connection, &context, change_set).await
    })?;

    // Respond with JSON if the thread was modified. Otherwise, return an empty
    // response with a 204 status code.
    if let Some(changed) = thread.changed() {
        // Generate the response with a reference to changed thread so we can
        // move the original value into the relay event.
        let response = Response::build().json(&changed)?;

        // Dispatch the "edit-thread" event to the subscribers in the channel.
        app.pubsub().dispatch(OurEvent::edit_thread(changed));

        Ok(response)
    } else {
        Response::build().status(StatusCode::NO_CONTENT).finish()
    }
}

/// Delete an existing thread by id.
async fn destroy(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let context = ThreadMemberContext::try_from(&request)?;

    if let 1.. = hurry!(async {
        // Checkout a database connection.
        let mut connection = request.app().database().get().await?;

        // Peform the delete.
        diesel::delete(threads::table)
            // Convert the context derived from the request into a boxed filter
            // expression that filters the delete by id, thread_id, and user_id.
            .filter(context.to_delete_where())
            .execute_async(&mut connection)
            .await
    })? {
        Response::build().status(StatusCode::NO_CONTENT).finish()
    } else {
        via::deny!(403, "insufficient permission")
    }
}
