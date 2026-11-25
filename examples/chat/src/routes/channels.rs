via::resource!(app = Unicorn, guard = collection);

use diesel::{BoolExpressionMethods, ExpressionMethods, Identifiable, QueryDsl};
use http::StatusCode;
use std::ops::ControlFlow;
use tokio::task::coop;
use via::{Payload, Response, ResultExt};
use via_diesel::paginate::PER_PAGE;
use via_diesel::{AsyncQueryDsl, LimitAndPage, Paginate};
use via_pubsub::{Dispatch, OurEvent};

use crate::models::subscription::{
    self, AuthorizedSubscription, ChannelSubscription, SubscriptionKeys,
};
use crate::models::thread::{self, ThreadDetails, ThreadWithUser};
use crate::models::{Channel, ChannelWithThreads, Reaction};
use crate::schema::{channels, subscriptions, threads};
use crate::util::{Id, Session, hurry};
use crate::{Next, Request, Unicorn};

#[derive(Clone)]
struct Protected(AuthorizedSubscription);

struct AuthorizationArgs {}

pub trait Subscriber {
    /// The authorized subscription to the channel with `id = :channel-id`.
    ///
    /// When `Some`, the current user has the minimum claims required to
    /// participate.
    fn subscription(&self) -> Option<&AuthorizedSubscription>;
}

/// List the channels to which the current user is subscribed.
async fn index(request: Request, _: Next) -> via::Result {
    // Get the id of the active user from the session.
    let me = request.me()?;

    // Source pagination arguments from the URI query.
    let by_page = request.query::<LimitAndPage<50>>()?;

    // Load the active user's subscriptions.
    let channels = {
        // Checkout a database connection.
        let mut connection = request.app().database().get().await?;

        ChannelSubscription::query()
            .filter(subscription::by_user(me))
            .page(by_page)
            .load_async(&mut connection)
            .await?
    };

    Response::build().data(&channels)
}

/// Create a new channel.
async fn create(request: Request, _: Next) -> via::Result {
    // Get the id of the current user from the request session.
    let me = request.me()?;

    // Deserialize a NewChannel from the request body.
    let (body, app) = request.into_future();
    let new_channel = body.timeout_after_secs(2).await?.data()?;

    // Insert the channel and associate it to the current user.
    let channel = hurry!(async {
        let mut connection = app.database().get().await?;
        Channel::create(&mut connection, me, new_channel).await
    })?;

    // Dispatch the "register" event so the current user receives update
    // notifications for changes that occur in `channel`.
    //
    // This notification must be dispatched before the response is generated.
    let event = OurEvent::register(Some(me), *channel.id());
    app.pubsub().dispatch(event);

    Response::build().status(StatusCode::CREATED).data(&channel)
}

pub async fn authorization(mut request: Request, next: Next) -> via::Result {
    // Group authorization dependencies in `SubscriptionKeys`.
    let keys = SubscriptionKeys::new(
        // Load the current user id from the request session.
        request.me()?,
        // Parse an Id from the :channel-id request URI path parameter.
        request.param("channel-id").parse()?,
    );

    // Fetch the current user's subscription to the channel.
    let subscription = hurry!(async {
        let mut connection = request.app().database().get().await?;
        AuthorizedSubscription::fetch(&mut connection, &keys).await
    })?;

    // Insert the AuthorizedSubscription into the request extensions.
    request.extensions_mut().insert(Protected(subscription));

    // Delegate to the next middleware.
    next.call(request).await
}

/// Find a channel by id.
async fn show(request: Request, _: Next) -> via::Result {
    // Clone the subscription we loaded during authorization.
    let subscription = request.subscription().cloned().or_not_found()?;

    // The first page of recent threads in the channel in `subscription`.
    let mut threads = {
        // Acquire a database connection.
        let mut connection = request.app().database().get().await?;

        // Query the database for the first page of recent threads.
        let threads =
            ThreadWithUser::recent_in_channel(&mut connection, subscription.channel_id()).await?;

        // Side load the top reactions to the threads in `threads`.
        Reaction::to_threads(&mut connection, threads).await?
    };

    // Reverse the order of threads to match their render sequence.
    threads.reverse();

    // Group `threads` within `subscription`.
    let channel = ChannelWithThreads::new(subscription, threads);

    Response::build().data(&channel)
}

/// Update a channel by id. Admin permissions are required.
async fn update(request: Request, _: Next) -> via::Result {
    // Extract SubscriptionKeys from the request subscription if the user is an
    // admin. Otherwise, 403 forbidden.
    let keys = request
        .subscription()
        .and_then(|subscription| subscription.is_admin().then(|| subscription.to_keys()))
        .or_forbidden()?;

    // Aggregate the request body and deserialize a channel change set.
    let (body, app) = request.into_future();
    let change_set = body.timeout_after_secs(2).await?.data()?;

    // Apply the change set to the channel.
    let channel = hurry!(async {
        // Acquire a database connection.
        let mut connection = app.database().get().await?;

        // Perform the update.
        Channel::update(&mut connection, keys.channel_id(), change_set).await
    })?;

    Response::build().data(&channel)
}

/// Delete a channel by id. Admin permissions are required.
async fn destroy(request: Request, _: Next) -> via::Result {
    // Extract SubscriptionKeys from the request subscription if the user is an
    // admin. Otherwise, 403 forbidden.
    let keys = request
        .subscription()
        .and_then(|subscription| subscription.is_admin().then(|| subscription.to_keys()))
        .or_forbidden()?;

    // Confirm that the user is a channel admin.
    if let 1.. = hurry!(async {
        // Checkout a database connection.
        let mut connection = request.app().database().get().await?;

        // Peform the delete.
        diesel::delete(channels::table)
            .filter(channels::id.eq(keys.channel_id()))
            .execute_async(&mut connection)
            .await
    })? {
        // Unsubscribe all users from the channel.
        request
            .app()
            .pubsub()
            .dispatch(OurEvent::deregister(None, *keys.channel_id()));

        Response::build().status(StatusCode::NO_CONTENT).finish()
    } else {
        via::deny!(404, "not found");
    }
}

impl Subscriber for Request {
    fn subscription(&self) -> Option<&AuthorizedSubscription> {
        if let Some(Protected(subscription)) = self.extensions().get() {
            Some(subscription)
        } else {
            None
        }
    }
}
