use diesel::Identifiable;
use serde::{Deserialize, Serialize};
use via_pubsub::OurEvent;

use crate::models::reaction::ReactionWithUser;
use crate::models::thread::{ThreadDelta, ThreadMemberContext, ThreadWithUser};
use crate::util::Id;

pub trait Relay: Sized {
    fn new_reply(thread: ThreadWithUser) -> Self;
    fn edit_reply(thread: ThreadWithUser) -> Self;
    fn delete_reply(context: &ThreadMemberContext) -> Self;
}

/// A change notification.
#[derive(Debug, Deserialize, Serialize)]
#[serde(content = "data", rename_all = "kebab-case", tag = "type")]
pub enum Notification {
    Reaction(ReactionWithUser),
    NewReply(ThreadWithUser),
    EditReply(ThreadDelta),
    DeleteReply { id: Id },
}

impl Relay for OurEvent<Id, Notification> {
    fn new_reply(thread: ThreadWithUser) -> Self {
        Self::relay(thread.channel_id(), Notification::NewReply(thread))
    }

    fn edit_reply(thread: ThreadWithUser) -> Self {
        Self::relay(
            thread.channel_id(),
            Notification::EditReply(thread.into_delta()),
        )
    }

    fn delete_reply(context: &ThreadMemberContext) -> Self {
        Self::relay(
            *context.channel_id(),
            Notification::DeleteReply {
                id: *context.reply_id().unwrap_or(context.thread_id()),
            },
        )
    }
}
