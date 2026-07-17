use serde::Deserialize;
use via::deny;
use via::error::Propagate;
use via::ws::{self, Channel, Message};
use via_pubsub::{Event, PeerEvent};

use crate::app::{Connection, Notification, Unicorn};
use crate::models::reaction::{NewReactionInChannel, Reaction};
use crate::models::thread::{NewThread, Thread};
use crate::models::user::User;
use crate::models::{ChannelSubscription, UserPreview};
use crate::util::{Id, Session};

type Request = via::ws::Request<Unicorn>;

#[derive(Deserialize)]
#[serde(content = "data", rename_all = "lowercase", tag = "type")]
enum ClientEvent {
    Reaction(NewReactionInChannel),
    Reply(NewThread),
}

pub async fn chat(mut channel: Channel, request: Request) -> ws::Result {
    log!(info(chat), "setup recv loop");

    // An authenticated user is required.
    let me = request.me().or_break()?;

    // Load a user preview for the current user along with their channels.
    let user = {
        // Acquire a database connection.
        let mut connection = request.app().database().await.or_break()?;

        // Execute the query.
        User::with_subscriptions(&mut connection, me)
            .await
            .or_break()?
    };

    // Get a subscription scoped to the current user.
    let mut subscription = request.app().pubsub().subscribe(me);

    // Register interest in the channels that the user is subscribed to.
    user.subscriptions()
        .map(ChannelSubscription::channel_id)
        .for_each(|interest| subscription.register(interest));

    // Start receiving messages from the client and peers.
    loop {
        tokio::select! {
            // client <- self <- peers
            result = subscription.recv() => {
                let Some(event) = result? else {
                    continue; // Event filtered by subscription.
                };

                match event {
                    // The user logged out.
                    PeerEvent::Logout(_) => {
                        log!(info(chat = 1), "ws session ended");
                        return Ok(()); // End the session.
                    }
                    // Notification received from a peer.
                    PeerEvent::Relay(_, notification) => {
                        channel.send(notification).await?
                    }
                    // The user was invited to a channel.
                    PeerEvent::Register(_, interest) => {
                        subscription.register(interest);
                    }
                    // The user was removed from a channel.
                    PeerEvent::Deregister(_, ref interest) => {
                        subscription.deregister(interest);
                    }
                }
            }

            // client -> self -> peers
            next = channel.recv() => {
                // Attempt to extract ClientEvent from the next message.
                let event = {
                    // Borrow the original message from `next`.
                    let option = next.as_ref();

                    // Attempt to extract ClientEvent from the next message.
                    if let Some(message) = option && message.is_text() {
                        // Deserialize a client event from `message`.
                        // If an error occurs, restart the listener.
                        let event = extract_client_event(message).or_continue()?;

                        // Drop the option containing the original message.
                        drop(next);

                        event
                    } else if let Some(message) = option && !message.is_close() {
                        log!(info(chat = 1), "unknown opcode received from client");
                        log!(info(chat = 2), "{:#?}", message);
                        continue; // Ignore unknown opcodes.
                    } else {
                        log!(info(chat = 1), "ws session ended");
                        return Ok(()); // End the session.
                    }
                };

                // Persist the client event and prepare to notify peers.
                let notification = {
                    // Acquire a database connection.
                    let mut connection = request.app().database().await.or_break()?;

                    // Perform the insert.
                    persist_client_event(&mut connection, user.to_preview(), event)
                        .await
                        .or_continue()? // If an occurs, restart `chat`.
                };

                // Publish the notification to subscribers.
                subscription.send(notification).await?;
            }
        }
    }
}

#[cfg(feature = "tokio-tungstenite")]
fn extract_client_event(message: &Message) -> via::Result<ClientEvent> {
    let text = message.to_text()?;
    Ok(serde_json::from_str(text)?)
}

#[cfg(feature = "tokio-websockets")]
fn extract_client_event(message: &Message) -> via::Result<ClientEvent> {
    let payload = message.as_payload();
    let text = str::from_utf8(payload)?;

    Ok(serde_json::from_str(text)?)
}

async fn persist_client_event(
    connection: &mut Connection<'_>,
    actor: UserPreview,
    event: ClientEvent,
) -> via::Result<Event<Id, Notification>> {
    match event {
        // Insert a thread into the threads table.
        ClientEvent::Reply(mut new_thread) => {
            // The authenticated user owns the thread.
            new_thread.user_id = Some(actor.id());

            // Store the channel id so we can use it as a pubsub interest.
            let Some(interest) = new_thread.channel_id else {
                deny!(500, "thread is missing required field channel_id");
            };

            // Perform the insert.
            let thread = Thread::create(connection, new_thread).await?;
            let thread = thread.with_user(actor);

            // Create a publishable event containing a reply notification
            // scoped to the channel.
            Ok(Event::relay(interest, Notification::Reply(thread)))
        }
        // Insert a reaction into the reactions table.
        ClientEvent::Reaction(mut new_reaction) => {
            // The authenticated user owns the reaction.
            new_reaction.user_id = Some(actor.id());

            // Store the channel id so we can use it as a pubsub interest.
            let interest = new_reaction.channel_id();

            // Perform the insert.
            let reaction = Reaction::create_in(connection, new_reaction).await?;
            let reaction = reaction.with_user(actor);

            // Create a publishable event containing a reaction notification
            // scoped to the channel.
            Ok(Event::relay(interest, Notification::Reaction(reaction)))
        }
    }
}
