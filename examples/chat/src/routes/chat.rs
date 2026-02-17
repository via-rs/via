use diesel::prelude::*;
use std::collections::HashMap;
use via::request::Payload;
use via::ws::{self, Channel, Request, ResultExt};

use crate::chat::{Chat, Event, EventContext};
use crate::models::conversation::{Conversation, NewConversation};
use crate::models::subscription::{AuthClaims, Subscription, by_user};
use crate::schema::{conversations, subscriptions};
use crate::util::{DebugQueryDsl, Id, Session};

/// Prints the format string to stderr in debug builds.
///
macro_rules! debug {
    ($($args:tt)+) => { if cfg!(debug_assertions) { eprintln!($($args)+); } };
}

pub async fn chat(mut channel: Channel, request: Request<Chat>) -> ws::Result {
    // The current user that opened the websocket.
    let user_id = request.user().cloned().or_break()?;

    // Subscribe to event notifications from peers.
    let mut pubsub = request.app().subscribe();

    // The current users channel subscription claims keyed by channel id.
    let subscriptions: HashMap<Id, AuthClaims> = {
        let acquire = request.app().database().await;
        let result = Subscription::query()
            .select((subscriptions::channel_id, subscriptions::claims))
            .filter(by_user(&user_id))
            .debug_load::<(Id, AuthClaims)>(&mut acquire.or_break()?)
            .await;

        result.or_break()?.into_iter().collect()
    };

    loop {
        let mut new_conversation = tokio::select! {
            // Received a message from the websocket channel.
            Some(message) = channel.recv() => {
                if message.is_close() {
                    debug!("info(chat): close requested by client");
                    return Ok(());
                }

                if !message.is_text() {
                    debug!("warn(chat): ignoring {:?}", message);
                    continue;
                }

                message.json::<NewConversation>().or_continue()?
            }

            // Received an event notification from another async task.
            Ok((ref event, message)) = pubsub.recv() => {
                if user_id != *event.user_id()
                    && let Some(id) = event.channel_id()
                    && let Some(claims) = subscriptions.get(id)
                    && claims.contains(AuthClaims::VIEW)
                {
                    channel.send(message).await?;
                }

                continue;
            }
        };

        // Confirm that the current user can write in the
        // channel before we proceed.
        if let Some(id) = &new_conversation.channel_id
            && let Some(claims) = subscriptions.get(id)
            && claims.contains(AuthClaims::WRITE)
        {
            new_conversation.user_id = Some(user_id);
        } else {
            continue;
        }

        // Acquire a database connection and create the message.
        let conversation = diesel::insert_into(conversations::table)
            .values(new_conversation)
            .returning(Conversation::as_returning())
            .debug_result(&mut request.app().database().await.or_continue()?)
            .await
            .or_continue()?;

        // Build the event context from the request and params. We use this to
        // determine if a message is for the current user in the second arm of
        // the select expression above.
        let data = EventContext::new(Some(*conversation.channel_id()), user_id);

        // Insert the message into the database and return a message event.
        let event = Event::Message(conversation);

        // Publish the event over the broadcast channel to notify peers.
        request.app().publish(data, event).or_continue()?;
    }
}
