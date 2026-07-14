use diesel::Identifiable;
use serde::{Deserialize, Serialize};
use via::error::Propagate;
use via::ws::{self, Channel, Message};
use via_diesel::prelude::*;

use crate::app::Unicorn;
use crate::models::reaction::{NewReactionInChannel, Reaction, ReactionWithUser};
use crate::models::thread::{self, NewThread, Thread, ThreadWithUser};
use crate::models::user::User;
use crate::schema::threads;
use crate::util::Session;
use crate::util::pubsub::{Action, Event, SharedStr};

type Request = via::ws::Request<Unicorn>;

#[derive(Deserialize)]
#[serde(content = "data", rename_all = "lowercase", tag = "type")]
enum ClientEvent {
    Reaction(NewReactionInChannel),
    Reply(NewThread),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(content = "data", rename_all = "lowercase", tag = "type")]
pub enum ClientUpdate {
    Reaction(ReactionWithUser),
    Reply(ThreadWithUser),
}

pub async fn chat(mut channel: Channel, request: Request) -> ws::Result {
    log!(info(chat), "setup recv loop");

    // Get an owned pubsub handle to comminucate with other unicorn instances.
    let mut pubsub = request.app().subscribe();

    // Load the active user along with the channels they are subscribed to.
    let me = {
        let future = async {
            // An authenticated user is required.
            let id = request.me()?;

            // Acquire a database connection.
            let connection = request.app().database().await?;

            // Execute the query.
            User::with_subscriptions(connection, id).await
        };

        future.await.or_break()?
    };

    // Register interest in the channels that the user is subscribed to.
    for subscription in me.subscriptions() {
        pubsub.subscribe(*subscription.channel().id());
    }

    // Start receiving messages from the client and peers.
    loop {
        tokio::select! {
            // client <- self <- peers
            result = pubsub.recv() => {
                let Some(event) = result? else {
                    continue; // Event filtered by pubsub handle.
                };

                match event.action() {
                    // Update received from peer.
                    Action::Update(update) => {
                        let update = update.clone();
                        channel.send(update).await?;
                    }
                    // The user was invited to a channel.
                    Action::Subscribe => {
                        pubsub.subscribe(event.interest());
                    }
                    // The user was removed from a channel.
                    Action::Unsubscribe => {
                        pubsub.subscribe(event.interest());
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

                // Persist the client event.
                let persist = async {
                    let (interest, update) = match event {
                        // Insert a thread into the threads table.
                        ClientEvent::Reply(mut new_thread) => {
                            // Acquire a database connection.
                            let mut connection = request.app().database().await?;

                            // The authenticated user owns the thread.
                            new_thread.user_id = Some(me.id());

                            // Perform the insert.
                            let thread = Thread::create(&mut connection, new_thread).await?;
                            let interest = thread.channel_id();

                            (interest, ClientUpdate::Reply(thread.with_user(me.to_preview())))
                        }

                        // Insert a reaction into the reactions table.
                        ClientEvent::Reaction(mut new_reaction) => {
                            // Acquire a database connection.
                            let mut connection = request.app().database().await?;

                            // The authenticated user owns the reaction.
                            new_reaction.user_id = Some(me.id());

                            // Confirm the thread is an channel that the user is subscribed to.
                            let interest = Thread::query()
                                .filter(thread::by_id(thread_id))
                                .select(threads::channel_id)
                                .first(&mut connection)
                                .await?;

                            // Perform the insert.
                            let reaction = Reaction::create(&mut connection, new_reaction).await?;

                            (interest, ClientUpdate::Reaction(reaction.with_user(me.to_preview())))
                        }
                    };

                    let payload = SharedStr::from(serde_json::to_string(&update)?);

                    Ok(Event::new(interest, Action::Update(payload)))
                };

                let event = persist.await.or_continue()?;

                pubsub.publish(event).await?;
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

impl ClientEvent {
    pub fn interest(&self) -> Id {
        match self {
            Self::Reaction(reaction) => reaction.channel_id(),
            Self::Reply(thread) => thread.channel_id,
        }
    }
}
