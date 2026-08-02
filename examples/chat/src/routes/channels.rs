via::resource!(app = Unicorn, guard = collection);

use diesel::{BoolExpressionMethods, Identifiable, QueryDsl};
use http::StatusCode;
use std::ops::ControlFlow;
use tokio::task::coop;
use via::{Payload, Response, ResultExt, deny};
use via_diesel::paginate::PER_PAGE;
use via_diesel::{AsyncQueryDsl, LimitAndPage, Paginate};
use via_pubsub::Event;

use crate::models::subscription::{self, AuthClaims, ChannelSubscription};
use crate::models::thread::{self, ThreadDetails, ThreadWithUser};
use crate::models::{Channel, Reaction};
use crate::schema::{channels, subscriptions};
use crate::util::{Id, Session};
use crate::{Next, Request, Unicorn};

#[derive(Clone)]
struct Protected(ChannelSubscription);

pub trait Subscriber {
    fn channel(&self) -> Option<&ChannelSubscription>;
    fn channel_id(&self) -> Option<Id> {
        self.channel().map(ChannelSubscription::channel_id)
    }
}

pub async fn authorization(mut request: Request, next: Next) -> via::Result {
    // Get the id of the user from the session.
    let me = request.me()?;

    // Parse an Id from the :channel-id path parameter.
    let id = request.param("channel-id").parse::<Id>()?;

    // Find the current user's subscription to the channel.
    let channel = {
        // Checkout a database connection.
        let mut connection = request.app().database().await?;

        // The user's subscription to the channel must exist and have the
        // minimum auth claims in order to proceed.
        let by_channel_and_user = subscription::by_channel(id)
            .and(subscription::by_user(me))
            .and(subscription::can_participate());

        // Execute the query.
        ChannelSubscription::query()
            .filter(by_channel_and_user)
            .first_async(&mut connection)
            .await?
    };

    // Insert the subscription to the channel into the request extensions.
    request.extensions_mut().insert(Protected(channel));

    // Delegate to the next middleware.
    next.call(request).await
}

/// List channels.
///
/// Responds to `GET /api/channels`.
async fn index(request: Request, _: Next) -> via::Result {
    // Get the id of the active user from the session.
    let me = request.me()?;

    // Source pagination arguments from the URI query.
    let by_page_number = request.query::<LimitAndPage>()?;

    // Load the active user's subscriptions.
    let channels = {
        // Checkout a database connection.
        let mut connection = request.app().database().await?;

        ChannelSubscription::query()
            .filter(subscription::by_user(me))
            .page(by_page_number)
            .load_async(&mut connection)
            .await?
    };

    Response::build().data(channels)
}

/// Create a new channel.
///
/// Responds to `POST /api/channels`.
async fn create(request: Request, _: Next) -> via::Result {
    // Get the id of the active user from the session.
    let me = request.me()?;

    // Deserialize a NewChannel from the request body.
    let (body, app) = request.into_future();
    let new_channel = body.timeout_after_secs(2).await?.data()?;

    // Insert the channel and associate it to the active user.
    let channel = {
        // Acquire a database connection.
        let mut connection = app.database().await?;

        // Insert the channel.
        Channel::create(&mut connection, me, new_channel).await?
    };

    // Create a "register" event with the current user id and channel id.
    let event = Event::register(Some(me), *channel.id());

    // Dispatch the "register" event so the current user receives update
    // notifications for changes that occur in `channel`.
    app.pubsub().dispatch(event);

    Response::build().status(StatusCode::CREATED).data(channel)
}

/// Find a channel by id.
///
/// Responds to `GET /api/channels/:channel-id`.
///
/// The active user must be a member of the channel.
async fn show(request: Request, _: Next) -> via::Result {
    // Clone the channel subscription we loaded during authorization.
    let subscription = request.channel().cloned().or_not_found()?;

    // Load the associations for the channel in `subscription`.
    let threads = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Get the id of the channel from `subscription`.
        let channel_id = subscription.channel_id();

        // Load the first page of the most recent threads in the channel.
        let mut recent = ThreadWithUser::query()
            .filter(thread::by_channel(channel_id).and(thread::is_thread()))
            .order(thread::recent())
            .limit(PER_PAGE)
            .load_async(&mut connection)
            .await?;

        // Reverse the first page of threads to match their render sequence.
        recent.reverse();

        // Side load the top reactions to the threads in `threads`.
        Reaction::to_threads(&mut connection, recent).await?
    };

    // Render `threads` within the channel subscription.
    Response::build().data(subscription.with_threads(threads))
}

/// Update an existing channel.
///
/// The active user must be a channel admin.
///
/// Responds to `PATCH /api/channels/:channel-id`.
async fn update(request: Request, _: Next) -> via::Result {
    // Clone the subscription we loaded during authorization.
    let subscription = request.channel().cloned().or_not_found()?;

    // Confirm that the user is a channel admin.
    if !subscription.claims().contains(AuthClaims::ADMINISTRATE) {
        deny!(403, "updating a channel requires a channel admin");
    }

    // Aggregate the request body and deserialize a channel change set.
    let (body, app) = request.into_future();
    let change_set = body.timeout_after_secs(2).await?.data()?;

    // Apply the change set to the channel.
    let channel = {
        // Acquire a database connection.
        let mut connection = app.database().await?;

        // Get the channel id from subscription.
        let channel_id = subscription.channel_id();

        // Perform the update.
        Channel::update(&mut connection, channel_id, change_set).await?
    };

    Response::build().data(channel)
}

/// Delete a channel.
///
/// The active user must be a channel admin.
///
/// Responds to `DELETE /api/channels/:channel-id`.
async fn destroy(request: Request, _: Next) -> via::Result {
    // Clone the subscription we loaded during authorization.
    let subscription = request.channel().cloned().or_not_found()?;

    // Confirm that the user is a channel admin.
    if !subscription.claims().contains(AuthClaims::ADMINISTRATE) {
        deny!(403, "deleting a channel requires a channel admin");
    } else {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Get the channel id from `subscription`.
        let id = subscription.channel_id();

        // Unsubscribe all users from the channel.
        let event = Event::deregister(None, id);
        request.app().pubsub().dispatch(event);

        // Destroy the channel.
        Channel::destroy(&mut connection, id).await?;
    }

    Response::build().status(StatusCode::NO_CONTENT).finish()
}

impl Subscriber for Request {
    fn channel(&self) -> Option<&ChannelSubscription> {
        self.extensions().get().map(|Protected(channel)| channel)
    }
}
