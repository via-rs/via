use serde::{Deserialize, Serialize};
use tokio::task::coop;
use via::deny;
use via::error::{Catch, Propagate};
use via::ws::{self, Channel, Message};
use via_pubsub::{Dispatch, OurEvent, PeerEvent, Publish, Receive, Scope};

use crate::app::{Connection, Notification, Postgres, Unicorn};
use crate::models::reaction::{NewReactionInChannel, Reaction};
use crate::models::thread::{NewThreadInChannel, Thread};
use crate::models::user::User;
use crate::models::{ChannelSubscription, UserPreview, subscription};
use crate::util::{Id, Session};

type Request = via::ws::Request<Unicorn>;

#[derive(Deserialize)]
#[serde(content = "data", rename_all = "lowercase", tag = "type")]
enum ClientEvent {
    Reply(NewThreadInChannel),
    Reaction(NewReactionInChannel),
}

#[derive(Serialize)]
#[serde(content = "data", rename_all = "lowercase", tag = "type")]
enum LagNotification {
    Lag { length: u64 },
}

macro_rules! hurry {
    ($future:expr) => {
        coop::unconstrained($future).await
    };
}

pub async fn chat(mut channel: Channel, request: Request) -> ws::Result {
    log!(info(chat), "setup recv loop");

    // An authenticated user is required.
    let me = request.me().or_break()?;

    // Borrow `&App` for the duration of this listener. We own a copy of
    // `request` and it is guaranteed a stable memory address by `Arc`.
    //
    // Therefore, we know that this borrow is valid for the lifetime of the
    // future returned by this function. Also, checking out a database
    // connection is not a callable function on `&App`.
    //
    // The risk of holding this borrow between awaits is considerably lower
    // than any of the alternatives.
    let app = request.app();

    // Get a subscription scoped to the current user.
    let mut pubsub = app.pubsub().subscribe(me);

    // Register interest in the channels that the user is subscribed to.
    for interest in hurry!(async {
        let mut connection = app.database().get().await.or_break()?;
        ChannelSubscription::participating(&mut connection, me).await
    })? {
        pubsub.register(interest);
    }

    // Start receiving messages from the client and peers.
    loop {
        let inbound = tokio::select! {
            // Assume that we just published an event.
            biased;

            // client <- self <- peers
            result = pubsub.recv() => result?,

            // client -> self -> peers
            outbound = channel.recv() => {
                // Attempt to extract an event from the next message.
                let event = match outbound {
                    // Try to deserialize a `ClientEvent` from text messages.
                    Some(message) if message.is_text() => {
                        log!(info(chat = 1), "event received from client");
                        ClientEvent::try_from(&message).or_continue()?
                    }
                    // Disconnect if it is a close message. Otherwise, continue.
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
                let notification = match event {
                    ClientEvent::Reply(mut new_reply) => {
                        // Set the user_id of the reply to the current user id.
                        new_reply.user_id = Some(me);

                        // Acquire a database connection and perform the insert.
                        hurry!(async {
                            let mut connection = app.database().get().await.or_break()?;
                            reply_to(&mut connection, new_reply).await.or_continue()
                        })?
                    }
                    ClientEvent::Reaction(mut new_reaction) => {
                        // Set the user_id of the reaction to the current user id.
                        new_reaction.user_id = Some(me);

                        // Acquire a database connection and perform the insert.
                        hurry!(async {
                            let mut connection = app.database().get().await.or_break()?;
                            react_to(&mut connection, new_reaction).await.or_continue()
                        })?
                    }
                };

                // Log the result of the database operation.
                //
                // This let's developers who are reading debug logs know that
                // we woke for the result of the `persist_client_event` future.
                log!(info(chat = 1), "event saved to database");

                // Publish the notification to subscribers.
                pubsub.send(notification).await?;

                // Notify the successful publish in debug builds.
                //
                // This is particularly helpful when you want to know whether
                // or not the reactor woke because a busy subscription caused
                // the publish future to yield before send.
                log!(info(chat = 2), "event published");

                // If an inbound event was received during the insert and we
                // have budget remaining, proceed with the inbound event flow.
                if coop::has_budget_remaining() {
                    pubsub.try_recv()?
                } else {
                    continue;
                }
            }
        };

        if let Some(event) = inbound {
            match event {
                // Lag detected in `subscription`.
                PeerEvent::Lag(length) => {
                    log!(info(chat = 1), "lag notification; len = {}", length);
                    let notification = LagNotification::Lag { length };
                    let message = serde_json::to_string(&notification).or_continue()?;

                    channel.send(message).await?;
                    return ws::restart();
                }

                // The user logged out.
                PeerEvent::Logout => {
                    log!(info(chat = 1), "ws session ended");
                    return Ok(()); // End the session.
                }

                // Notification received from a peer.
                PeerEvent::Relay(notification) => {
                    channel.send(notification).await?;
                }

                // The user was invited to a channel.
                PeerEvent::Register(interest) => {
                    log!(info(chat = 1), "joining channel {}", interest);
                    pubsub.register(interest);
                }

                // The user was removed from a channel.
                PeerEvent::Deregister(ref interest) => {
                    log!(info(chat = 1), "leaving channel {}", interest);
                    pubsub.deregister(interest);
                }
            }
        }
    }
}

#[inline]
async fn react_to(
    connection: &mut Connection<'_>,
    new_reaction: NewReactionInChannel,
) -> via::Result<OurEvent<Id, Notification>> {
    let interest = new_reaction.channel_id;
    let notification = Reaction::create(connection, new_reaction).await?.into();

    Ok(OurEvent::relay(interest, notification))
}

#[inline]
async fn reply_to(
    connection: &mut Connection<'_>,
    new_reply: NewThreadInChannel,
) -> via::Result<OurEvent<Id, Notification>> {
    let interest = new_reply.channel_id;
    let notification = Thread::create(connection, new_reply).await?.into();

    Ok(OurEvent::relay(interest, notification))
}

impl TryFrom<&'_ Message> for ClientEvent {
    type Error = via::Error;

    #[cfg(all(feature = "tokio-tungstenite", not(feature = "tokio-websockets")))]
    fn try_from(message: &'_ Message) -> Result<Self, Self::Error> {
        let text = message.to_text()?;
        Ok(serde_json::from_str(text)?)
    }

    #[cfg(all(feature = "tokio-websockets", not(feature = "tokio-tungstenite")))]
    fn try_from(message: &'_ Message) -> Result<Self, Self::Error> {
        let text = message.to_text()?;
        Ok(serde_json::from_str(text)?)
    }
}
