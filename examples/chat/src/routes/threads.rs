via::resource!(app = Unicorn);

use diesel::{BoolExpressionMethods, BoxableExpression, ExpressionMethods, QueryDsl};
use http::StatusCode;
use serde::Serialize;
use via::{Error, Payload, Response, ResultExt};
use via_diesel::paginate::{Keyset, LimitAndOffset, PER_PAGE};
use via_diesel::{AsyncQueryDsl, Paginate};
use via_pubsub::{Dispatch, OurEvent};

use crate::models::thread::{self, CreateThreadParams, NewThread, ThreadParams};
use crate::models::{Reaction, Subscription, Thread, ThreadDetails, ThreadWithUser};
use crate::routes::channels::Subscriber;
use crate::util::{Id, Iso8601, Session, hurry};
use crate::{Next, Request, Unicorn, schema::threads};

async fn index(request: Request, _: Next) -> via::Result {
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

async fn create(request: Request, _: Next) -> via::Result {
    // Initialize the contextual parameters for creating a thread from the
    // channel subscription for which the request was authorized.
    let mut params = request
        .channel()
        .map(|channel| CreateThreadParams::try_from(channel.subscription()))
        .or_forbidden()??;

    // If a thread-id was supplied as a path parameter use it as the parent_id.
    params.parent_id = request.param("thread-id").ok_and_then(|id| id.parse())?;

    // Deserialize a NewThread from the request body.
    let (body, app) = request.into_future();
    let new_thread = body.timeout_after_secs(2).await?.data::<NewThread>()?;

    // Apply the context from the path parameters to the new thread.
    let new_thread = new_thread.in_channel(params);

    // Checkout a database connection and perform the insert.
    let thread = hurry!(async {
        let mut connection = app.database().get().await?;
        Thread::create(&mut connection, &new_thread).await
    })?;

    // Generate the response with a reference to `thread` so we can move the
    // original value into the relay event.
    let response = Response::build()
        .status(StatusCode::CREATED)
        .data(&thread)?;

    // Create a "relay" event with the newly created thread.
    let event = OurEvent::relay(thread.channel_id(), thread.into());

    // Dispatch the "relay" event to the subscribers in the channel.
    app.pubsub().dispatch(event);

    Ok(response)
}

async fn show(request: Request, _: Next) -> via::Result {
    // Parse an Id from the :thread-id path parameter.
    let params = request.params::<ThreadParams>()?;

    // Prefer branching before checking out a database connection.
    let thread = match params.reply_id.as_ref() {
        // Find an individual reply to a thread.
        Some(id) => {
            let mut connection = request.app().database().get().await?;
            ThreadDetails::find_reply(&mut connection, id).await?
        }
        // Find a thread and the latest page of replies.
        None => {
            let mut connection = request.app().database().get().await?;
            ThreadDetails::find_thread(&mut connection, &params.thread_id).await?
        }
    };

    Response::build().data(&thread)
}

/// Update an existing thread.
///
/// The current user must be authorized to access the channel to which the
/// thread belongs and own the thread. Otherwise, `404`.
async fn update(request: Request, _: Next) -> via::Result {
    // Deserialize the remaining delete arguments from the request URI path.
    let mut params = request.params::<ThreadParams>()?;

    // Set the user_id to that of the subscription resolved during authorization.
    params.user_id = request.channel().map(|c| c.subscription().user_id());

    // Aggregate the request body and deserialize a thread change set.
    let (body, app) = request.into_future();
    let change_set = body.timeout_after_secs(2).await?.data()?;

    // Checkout a database connection and update the thread.
    let thread = hurry!(async {
        let mut connection = app.database().get().await?;
        Thread::update(&mut connection, &params, &change_set).await
    })?;

    // Respond with JSON if the thread was modified. Otherwise, return an empty
    // response with a 204 status code.
    if thread.did_change() {
        Response::build().json(&thread)
    } else {
        Response::build().status(StatusCode::NO_CONTENT).finish()
    }
}

/// Delete an existing thread by id.
async fn destroy(request: Request, _: Next) -> via::Result {
    // The current user id comes from the subscription for which the request
    // was authorized.
    let me = request
        .channel()
        .map(|channel| channel.subscription().user_id())
        .or_forbidden()?;

    // Deserialize the remaining delete arguments from the request URI path.
    let mut params = request.params::<ThreadParams>()?;

    // Set the user_id of params to the current user id.
    params.user_id = Some(me);

    // Checkout a database connection and delete the thread.
    if let 1.. = hurry!(async {
        if let Some(id) = params.reply_id.as_ref() {
            let mut connection = request.app().database().get().await?;
            let by_id_and_thread_id = threads::id
                .eq(id)
                .and(threads::thread_id.eq(&params.thread_id));

            diesel::delete(threads::table)
                .filter(by_id_and_thread_id)
                .execute_async(&mut connection)
                .await
        } else {
            let mut connection = request.app().database().get().await?;
            let by_id_with_no_owner = threads::id
                .eq(&params.thread_id)
                .and(threads::thread_id.is_null());

            diesel::delete(threads::table)
                .filter(by_id_with_no_owner)
                .execute_async(&mut connection)
                .await
        }
    })? {
        Response::build().status(StatusCode::NO_CONTENT).finish()
    } else {
        via::deny!(403, "Forbidden")
    }
}
