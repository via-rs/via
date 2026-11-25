use diesel::Identifiable;
use serde::{Deserialize, Serialize};
use via_pubsub::OurEvent;

use crate::models::reaction::{ReactionMemberContext, ReactionWithUser};
use crate::models::thread::{ThreadDelta, ThreadMemberContext, ThreadWithUser};
use crate::util::Id;

pub trait Relay: Sized {
    fn new_reply(thread: ThreadWithUser) -> Self;
    fn edit_reply(thread: ThreadWithUser) -> Self;
    fn delete_reply(context: &ThreadMemberContext) -> Self;

    fn new_reaction(interest: &Id, thread: ReactionWithUser) -> Self;
    fn delete_reaction(context: &ReactionMemberContext) -> Self;
}

/// A change notification.
#[derive(Debug, Deserialize, Serialize)]
#[serde(content = "data", rename_all = "kebab-case", tag = "type")]
pub enum Notification {
    NewReply(ThreadWithUser),
    EditReply(ThreadDelta),
    DeleteReply { id: Id },

    NewReaction(ReactionWithUser),
    DeleteReaction { id: Id },
}

impl Relay for OurEvent<Id, Notification> {
    fn new_reply(thread: ThreadWithUser) -> Self {
        Self::relay(*thread.channel_id(), Notification::NewReply(thread))
    }

    fn edit_reply(thread: ThreadWithUser) -> Self {
        Self::relay(
            *thread.channel_id(),
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

    fn new_reaction(interest: &Id, reaction: ReactionWithUser) -> Self {
        Self::relay(*interest, Notification::NewReaction(reaction))
    }

    fn delete_reaction(context: &ReactionMemberContext) -> Self {
        Self::relay(
            *context.channel_id(),
            Notification::DeleteReaction {
                id: *context.reaction_id(),
            },
        )
    }
}
