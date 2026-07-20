use serde::Deserialize;
use via::deny;
use via::error::{Catch, Propagate};
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
    Reply(NewThread),
    Reaction(NewReactionInChannel),
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
        let peer_event = tokio::select! {
            // client -> self -> peers
            outbound = channel.recv() => {
                // Attempt to extract an event from the next message.
                let client_event = match outbound {
                    // Ignore binary messages.
                    #[cfg(debug_assertions)]
                    Some(message) if message.is_binary() => {
                        // Placeholder for tracing...
                        handle_binary_message(&message);
                        continue;
                    }

                    // Deserialize a client event from `message`.
                    Some(message) if message.is_text() => {
                        log!(info(chat = 1), "event received from client");
                        deserialize_client_event(&message).or_continue()?
                        //                                 ^^^^^^^^^^^
                        //          Restart `chat` if an error occurs.
                    }

                    // All other messages are control codes.
                    // End the session if Message::is_close. Otherwise, continue.
                    other => {
                        if other.as_ref().is_none_or(Message::is_close) {
                            log!(info(chat = 1), "ws session ended");
                            return Ok(()); // End the session.
                        } else {
                            continue;
                        }
                    }
                };

                // Persist the client event and prepare to notify peers.
                let notification = {
                    // Acquire a database connection.
                    let mut connection = request.app().database().await.or_break()?;
                    //                                                  ^^^^^^^^
                    // If we are unable to connect to the database, end the
                    // session.
                    //
                    // This allows us to implement reconnect logic on the
                    // client to find a healthy node.

                    // Get a user preview from the authenticated user.
                    //
                    // This allows us to include the users name and avatar in
                    // the update notification.
                    let actor = user.to_preview();

                    // Perform the insert.
                    //
                    // This is likely the first await point in the outbound
                    // flow where we'll have to yield to runtime.
                    persist_client_event(&mut connection, actor, client_event).await.or_continue()?
                };

                // Log the result of the database operation.
                //
                // This let's developers who are reading debug logs know that
                // we woke for the result of the `persist_client_event` future.
                log!(info(chat = 1), "event saved to database");

                // Publish the notification to subscribers.
                subscription.send(notification).await?;

                // Notify the successful publish in debug builds.
                //
                // This is particularly helpful when you want to know whether
                // or not the reactor woke because a busy subscription caused
                // the publish future to yield before send.
                log!(info(chat = 2), "event published");

                // If an inbound event was received while we were busy,
                // proceed with the inbound event flow.
                subscription.try_recv()?
            }

            // client <- self <- peers
            inbound = subscription.recv() => {
                inbound?
            }
        };

        match peer_event {
            // Event filtered by subscription.
            None => {}

            // The user logged out.
            Some(PeerEvent::Logout) => {
                log!(info(chat = 1), "ws session ended");
                return Ok(()); // End the session.
            }

            // Notification received from a peer.
            Some(PeerEvent::Relay(notification)) => {
                log!(info(chat = 1), "relay notification");
                channel.send(notification).await?
            }

            // The user was invited to a channel.
            Some(PeerEvent::Register(interest)) => {
                log!(info(chat = 1), "joining channel {}", interest);
                subscription.register(interest);
            }

            // The user was removed from a channel.
            Some(PeerEvent::Deregister(ref interest)) => {
                log!(info(chat = 1), "leaving channel {}", interest);
                subscription.deregister(interest);
            }
        }
    }
}

#[cfg(feature = "tokio-tungstenite")]
fn deserialize_client_event(message: &Message) -> via::Result<ClientEvent> {
    let text = message.to_text()?;
    Ok(serde_json::from_str(text)?)
}

#[cfg(feature = "tokio-websockets")]
fn deserialize_client_event(message: &Message) -> via::Result<ClientEvent> {
    let payload = message.as_payload();
    let text = str::from_utf8(payload)?;

    Ok(serde_json::from_str(text)?)
}

#[cfg(all(debug_assertions, feature = "tokio-tungstenite"))]
fn handle_binary_message(message: &Message) {
    log!(
        info(chat = 1),
        "ignoring binary message (len: {})",
        message.len()
    );
}

#[cfg(all(debug_assertions, feature = "tokio-websockets"))]
fn handle_binary_message(message: &Message) {
    log!(
        info(chat = 1),
        "ignoring binary message (len: {})",
        message.as_payload().len()
    );
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
