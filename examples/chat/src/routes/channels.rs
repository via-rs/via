via::resource!(app = Unicorn, guard = collection);

use serde::Serialize;
use via::request::PathParams;
use via::{Error, Payload, Response};
use via_diesel::LimitAndOffset;
use via_diesel::prelude::*;

use crate::models::subscription::{self, ChannelSubscription};
use crate::models::thread::{self, ThreadDetails, ThreadWithUser};
use crate::models::{Channel, Reaction};
use crate::schema::{channels, subscriptions};
use crate::util::{Id, Session};
use crate::{Next, Request, Unicorn};

#[derive(Clone, Debug, Serialize)]
pub struct ChannelMemberParams {
    pub channel_id: Id,
    pub org_id: Id,
}

pub async fn authorization(request: Request, next: Next) -> via::Result {
    next.call(request).await
}

/// List channels.
///
/// Responds to `GET /api/channels`.
async fn index(request: Request, _: Next) -> via::Result {
    // Get the id of the active user from the session.
    let me = request.me()?;

    // Get pagination params from the URI query.
    let limit_and_offset = request.query::<LimitAndOffset>()?;

    // Load the active user's subscriptions.
    let channels = {
        // Acquire a database connection.
        let mut conn = request.app().database().await?;

        // Execute the query.
        subscriptions::table
            .inner_join(channels::table)
            .select(Channel::as_select())
            .filter(subscription::by_user(&me))
            .order(subscription::recent())
            .page(limit_and_offset)
            .load(&mut conn)
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

    Response::build().status(201).data(channel)
}

/// Find a channel by id.
///
/// Responds to `GET /api/channels/:channel-id`.
///
/// The active user must be a member of the channel.
async fn show(request: Request, _: Next) -> via::Result {
    // Get the id of the active user from the session.
    let me = request.me()?;

    // Parse an Id from the :channel-id path parameter.
    let id = request.param("channel-id").parse::<Id>()?;

    let channel = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        let subscription = ChannelSubscription::query()
            .filter(subscription::by_user(&me).and(subscription::by_channel(&id)))
            .filter(subscription::claims_can_participate())
            .first(&mut connection)
            .await?;

        // Load the first page of recent messages in the channel.
        let threads = ThreadWithUser::query()
            .filter(thread::by_channel(&id).and(thread::is_thread()))
            .order(thread::recent())
            .limit(25)
            .load(&mut connection)
            .await?;

        // Load the reactions for the threads in threads.
        let threads = {
            //
            let ids = threads.iter().map(Identifiable::id).copied().collect();

            // Load the reactions for each message in the first page.
            let reactions = Reaction::to_threads(&mut connection, ids).await?;

            // Group reactions by their threads.
            ThreadDetails::grouped_by(threads, reactions)
        };

        subscription.with_threads(threads)
    };

    Response::build().data(channel)
}

/// Update an existing user.
///
/// Responds to `PATCH /api/channels/:channel-id`.
///
/// The active user must be a channel admin.
async fn update(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

/// Delete a user account.
///
/// Responds to `DELETE /api/channels/:channel-id`.
///
/// The active user must be a channel admin.
async fn destroy(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

impl<'a> TryFrom<PathParams<'a>> for ChannelMemberParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            channel_id: params.get("channel-id").parse()?,
            org_id: params.get("org-id").parse()?,
        })
    }
}
