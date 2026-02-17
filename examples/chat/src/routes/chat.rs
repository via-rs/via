use diesel::prelude::*;
use std::collections::HashMap;
use via::request::Payload;
use via::ws::{self, Channel, Message, Request, ResultExt};

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

pub async fn chat(mut socket: Channel, request: Request<Chat>) -> ws::Result {
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

    'recv: loop {
        let new_convo = 'send: {
            tokio::select! {
                // Received a message from the websocket channel.
                Some(message) = socket.recv() => match message {
                    Message::Text(payload) => {
                        let mut new_convo = payload.json::<NewConversation>().or_continue()?;

                        // Confirm that the current user can write in the
                        // channel before we proceed.
                        if let Some(id) = &new_convo.channel_id
                            && let Some(claims) = subscriptions.get(id)
                            && claims.contains(AuthClaims::WRITE)
                        {
                            new_convo.user_id = Some(user_id);
                            break 'send new_convo;
                        }
                    }
                    Message::Close(close) => {
                        close.inspect(|context| debug!("{:?}", context));
                        break 'recv Ok(());
                    }
                    Message::Binary(_) => {
                        debug!("warn(chat): ignoring binary message");
                    }
                    ignored => {
                        debug!("warn(chat): ignoring {:?}", ignored);
                    }
                },

                // Received an event notification from another async task.
                Ok((ref context, message)) = pubsub.recv() => {
                    if user_id != *context.user_id()
                        && let Some(id) = context.channel_id()
                        && let Some(claims) = subscriptions.get(id)
                        && claims.contains(AuthClaims::VIEW)
                    {
                        socket.send(message).await?;
                    }
                }
            }

            continue 'recv;
        };

        // Build the event context from the request and params. We use this to
        // determine if a message is for the current user in the second arm of
        // the select expression above.
        let context = EventContext::new(new_convo.channel_id, user_id);

        // Insert the message into the database and return a message event.
        let event = {
            // Acquire a database connection and create the message.
            let acquire = request.app().database().await;
            let create = diesel::insert_into(conversations::table)
                .values(new_convo)
                .returning(Conversation::as_returning())
                .debug_result(&mut acquire.or_continue()?)
                .await;

            Event::Message(create.or_continue()?)
        };

        // Publish the event over the broadcast channel to notify peers.
        request.app().publish(context, event).or_continue()?;
    }
}
