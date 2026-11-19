use diesel::Identifiable;
use serde::{Deserialize, Serialize};
use via_pubsub::OurEvent;

use crate::models::reaction::ReactionWithUser;
use crate::models::thread::{ThreadDelta, ThreadWithUser};
use crate::util::Id;

pub trait Relay: Sized {
    fn new_reply(thread: ThreadWithUser) -> Self;
    fn edit_thread(thread: ThreadWithUser) -> Self;
}

/// A change notification.
#[derive(Debug, Deserialize, Serialize)]
#[serde(content = "data", rename_all = "kebab-case", tag = "type")]
pub enum Notification {
    Reaction(ReactionWithUser),
    NewReply(ThreadWithUser),
    EditThread(ThreadDelta),
}

impl Relay for OurEvent<Id, Notification> {
    fn new_reply(thread: ThreadWithUser) -> Self {
        Self::relay(thread.channel_id(), Notification::NewReply(thread))
    }

    fn edit_thread(thread: ThreadWithUser) -> Self {
        Self::relay(
            thread.channel_id(),
            Notification::EditThread(thread.into_delta()),
        )
    }
}
