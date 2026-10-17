use serde::{Deserialize, Serialize};
use std::ops::ControlFlow;
use tokio::task::{coop, yield_now};
use via::error::{Catch, Propagate};
use via::ws::{self, Channel, Message};
use via::{Shared, deny};
use via_pubsub::{Dispatch, OurEvent, PeerEvent, Publish, Receive, Scope};

use crate::app::{Connection, Notification, Postgres, Unicorn};
use crate::models::reaction::{NewReactionInChannel, Reaction};
use crate::models::thread::{NewThreadInChannel, Thread};
use crate::models::user::User;
use crate::models::{ChannelSubscription, UserPreview, subscription};
use crate::util::{Id, Session, hurry};

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

macro_rules! relay {
    ($client:ident, $scope:ident, $event:expr) => {
        match $event {
            // Lag detected in `subscription`.
            PeerEvent::Lag(length) => {
                log!(info(chat = 1), "lag notification; len = {}", length);

                let notification = LagNotification::Lag { length };
                let message = serde_json::to_string(&notification).or_continue()?;

                Channel::send(&mut $client, message).await?;

                return ws::restart();
            }

            // The user logged out.
            PeerEvent::Logout => {
                log!(info(chat = 1), "ws session ended");
                return ws::disconnect(); // End the session.
            }

            // Notification received from a peer.
            PeerEvent::Relay(notification) => {
                Channel::send(&mut $client, notification).await?;
            }

            // The user was invited to a channel.
            PeerEvent::Register(interest) => {
                log!(info(chat = 1), "joining channel {}", interest);
                Scope::register(&mut $scope, interest);
            }

            // The user was removed from a channel.
            PeerEvent::Deregister(ref interest) => {
                log!(info(chat = 1), "leaving channel {}", interest);
                Scope::deregister(&mut $scope, interest);
            }
        }
    };
}

pub async fn join(mut client: Channel, request: Request) -> ws::Result {
    log!(info(chat), "setup recv loop");

    // Setup the chat session while yielding to the runtime as little as
    // possible.
    //
    // This async block performs the following:
    //
    //   - A scoped pubsub subscription is created with the current user's id
    //     as the "actor"
    //
    //   - A list of ids containing authenticated user's subscriptions are
    //     fetched from the database and registered as an interest
    //
    //   - The next event of the pubsub subscription is eagerly received and
    //     processed in the event that we are recovering from lag
    //
    // Each time an error occurs, due to lag or otherwise the user's
    // subscriptions are rebuilt from scratch. This makes lag recovery
    // indistinguishable in every way from a new chat session.
    let mut pubsub = hurry!(async {
        let unicorn = request.app();
        let mut scope = unicorn.pubsub().subscribe(request.me().or_break()?);
        let subscriptions = {
            let mut connection = unicorn.database().get().await.or_break()?;
            ChannelSubscription::participating(&mut connection, *scope.actor()).await?
        };

        // Register interest in the channels that the user is subscribed to.
        for interest in subscriptions {
            scope.register(interest);
        }

        // Try to eagerly receive and process the next peer event without
        // yielding.
        //
        // This prevents a positive feedback loop from occurring when trying to
        // recover from lag.
        if let Some(event) = scope.try_recv()? {
            relay!(client, scope, event);

            // Eagerly relaying a peer event requires an unconditional yield.
            yield_now().await;
        }

        Ok(scope)
    })?;

    // Work with an owned copy of our application `Shared<Unicorn>`.
    //
    // This shortens the deref chain required to checkout a database connection
    // for each iteration of the following dispatch loop.
    //
    // This atomic operation must be performed after your ws listener has fully
    // recovered from lag as it can contend with every other request.
    let unicorn = request.app_owned();

    log!(info(chat), "listening for client and peer events");

    // Start receiving messages from the client and peers.
    loop {
        tokio::select! {
            // peers -> self -> client
            inbound = pubsub.recv() => {
                if let Some(event) = inbound? {
                    relay!(client, pubsub, event);
                }
            }

            // client -> self -> peers
            outbound = client.recv() => {
                // Attempt to extract an event from the next message.
                if let Some(event) = outbound.as_ref().map_or_else(ws::disconnect, de_text)? {
                    log!(info(chat = 1), "event received from client");

                    // Persist the client event and prepare to notify peers.
                    let notification = event.persist(pubsub.actor(), &unicorn).await?;

                    log!(info(chat = 2), "event persisted to database");

                    // Publish the notification to subscribers.
                    pubsub.send(notification).await?;

                    // Notify the successful publish in debug builds.
                    //
                    // This is particularly helpful when you want to know whether
                    // or not the reactor woke because a busy subscription caused
                    // the publish future to yield before send.
                    log!(info(chat = 2), "event published");
                }

                // If the task budget remains, optimistically relay a peer event.
                if coop::has_budget_remaining() {
                    if let Some(event) = pubsub.try_recv()? {
                        relay!(client, pubsub, event);
                    }
                }
            }
        }
    }
}

fn de_text(message: &Message) -> ws::Result<Option<ClientEvent>> {
    if message.is_text() {
        Ok(Some(ClientEvent::try_from(message).or_continue()?))
    } else if message.is_close() {
        log!(info(chat = 1), "ws session ended");
        ws::disconnect()
    } else {
        Ok(None)
    }
}

impl ClientEvent {
    async fn persist(
        self,
        actor: &Id,
        unicorn: &Unicorn,
    ) -> ws::Result<OurEvent<Id, Notification>> {
        match self {
            ClientEvent::Reply(mut init) => {
                // Set the user_id of the reply to the current user id.
                init.user_id = Some(*actor);

                // Copy channel_id so we can use it again as the event interest.
                let interest = init.channel_id;

                // Acquire a database connection and perform the insert.
                let thread = hurry!(async {
                    let mut connection = unicorn.database().get().await.or_break()?;
                    Thread::create(&mut connection, init).await.or_continue()
                })?;

                Ok(OurEvent::relay(interest, thread.into()))
            }
            ClientEvent::Reaction(mut init) => {
                // Set the user_id of the reaction to the current user id.
                init.user_id = Some(*actor);

                // Copy channel_id so we can use it again as the event interest.
                let interest = init.channel_id;

                // Acquire a database connection and perform the insert.
                let reaction = hurry!(async {
                    let mut connection = unicorn.database().get().await.or_break()?;
                    Reaction::create(&mut connection, init).await.or_continue()
                })?;

                Ok(OurEvent::relay(interest, reaction.into()))
            }
        }
    }
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
