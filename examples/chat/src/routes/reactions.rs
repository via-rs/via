via::resource!(app = Unicorn, only = [collection, show, destroy]);

use diesel::{BoolExpressionMethods, ExpressionMethods, Identifiable, QueryDsl};
use http::StatusCode;
use via::request::{PathParams, Payloadz};
use via::{Payload, Response, ResultExt, deny};
use via_diesel::paginate::{LimitAndOffset, LimitAndPage};
use via_diesel::{AsyncQueryDsl, Paginate};
use via_pubsub::{Dispatch, OurEvent};

use super::chat::Relay;
use crate::models::reaction::{
    NewReaction, ReactionCollectionContext, ReactionMemberContext, WithReactedToId,
};
use crate::models::thread::ThreadParams;
use crate::models::{Reaction, ReactionWithUser};
use crate::routes::channels::Subscriber;
use crate::schema::{channels, reactions, threads};
use crate::util::{Authenticator, Id, Session, hurry};
use crate::{Next, Request, Unicorn};

/// List reactions to a thread or reply.
async fn index(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let context = ReactionCollectionContext::try_from(&request)?;

    // Extract pagination arguments from the URI query.
    let by_page = request.query::<LimitAndPage>()?;

    // Load a page of reactions to the thread or reply id in context.
    let reactions = hurry!(async {
        let mut connection = request.app().database().get().await?;
        ReactionWithUser::page_by_context(&mut connection, &context, by_page).await
    })?;

    Response::build().data(&reactions)
}

/// Create a new reaction to a thread.
async fn create(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let context = ReactionCollectionContext::try_from(&request)?;

    // Deserialize a NewReaction from the request body.
    let (body, app) = request.into_future();
    let new_reaction = body.timeout_after_secs(2).await?.data::<NewReaction>()?;

    // Checkout a database connection and insert the reaction.
    let reaction = hurry!(async {
        let mut connection = app.database().get().await?;
        Reaction::create(&mut connection, &context, new_reaction).await
    })?;

    // Generate the response with a reference to `thread` so we can move the
    // original value into the relay event.
    let response = Response::build()
        .status(StatusCode::CREATED)
        .data(&reaction)?;

    // Dispatch the "new-reaction" event to the subscribers in the channel.
    let event = OurEvent::new_reaction(context.channel_id(), reaction);
    app.pubsub().dispatch(event);

    Ok(response)
}

/// Retrieve a reaction to a thread or reply by id.
async fn show(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let context = ReactionMemberContext::try_from(&request)?;

    // Find the reaction with `id = :reaction-id` with context.
    let reaction = hurry!(async {
        let mut connection = request.app().database().get().await?;
        ReactionWithUser::find_by_context(&mut connection, &context).await
    })?;

    Response::build().data(&reaction)
}

/// Delete a reaction by id.
async fn destroy(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let context = ReactionMemberContext::try_from(&request)?;

    if let 1.. = hurry!(async {
        // Checkout a database connection.
        let mut connection = request.app().database().get().await?;

        // Deletes must be performed from the thread id path prefix that owns
        // the reaction by the user to which the reaction belongs.
        let by_channel_thread_user_and_id = reactions::thread_id
            .eq(context.reacted_to_id())
            .and(reactions::user_id.eq(context.user_id()))
            .and(reactions::id.eq(context.reaction_id()))
            .and(diesel::dsl::exists(
                threads::table
                    .filter(threads::id.eq(reactions::thread_id))
                    .filter(threads::channel_id.eq(context.channel_id())),
            ));

        // Peform the delete.
        diesel::delete(reactions::table)
            .filter(by_channel_thread_user_and_id)
            .execute_async(&mut connection)
            .await
    })? {
        let response = Response::build().status(StatusCode::NO_CONTENT).finish()?;

        // Dispatch the "delete-reaction" event to the subscribers in the channel.
        let event = OurEvent::delete_reaction(&context);
        request.app().pubsub().dispatch(event);

        Ok(response)
    } else {
        via::deny!(404, "not found")
    }
}
