use delegate::delegate;
use via::ResultExt;
use via::{Error, request::PathParams};

use crate::Request;
use crate::models::subscription::SubscriptionKeys;
use crate::models::thread::{ThreadMemberContext, ThreadParams};
use crate::routes::channels::Subscriber;
use crate::util::Id;

pub type ReactionCollectionContext = ThreadMemberContext;

pub trait WithReactedToId {
    fn reacted_to_id(&self) -> &Id;
}

pub struct ReactionMemberContext {
    params: ReactionParams,
    keys: SubscriptionKeys,
}

#[derive(Debug)]
pub struct ReactionParams {
    thread: ThreadParams,
    reaction_id: Id,
}

impl WithReactedToId for ReactionCollectionContext {
    delegate! {
        to self.params() {
            fn reacted_to_id(&self) -> &Id;
        }
    }
}

impl ReactionMemberContext {
    delegate! {
        to self.keys() {
            pub fn channel_id(&self) -> &Id;
            pub fn user_id(&self) -> &Id;
        }

        to self.params() {
            fn thread(&self) -> &ThreadParams;
        }
    }

    pub fn reaction_id(&self) -> &Id {
        &self.params().reaction_id
    }

    fn keys(&self) -> &SubscriptionKeys {
        &self.keys
    }

    fn params(&self) -> &ReactionParams {
        &self.params
    }
}

impl WithReactedToId for ReactionMemberContext {
    delegate! {
        to self.thread() {
            fn reacted_to_id(&self) -> &Id;
        }
    }
}

impl<'a> TryFrom<&'a Request> for ReactionMemberContext {
    type Error = Error;

    fn try_from(request: &'a Request) -> Result<Self, Self::Error> {
        Ok(Self {
            // Load the channel subscription that we resolved during authorization.
            keys: request
                .channel()
                .map(|channel| channel.keys())
                .or_forbidden()?,

            // Extract the thread params from the request URI path.
            params: request.params()?,
        })
    }
}

impl ReactionParams {
    fn thread(&self) -> &ThreadParams {
        &self.thread
    }
}

impl<'a> TryFrom<PathParams<'a>> for ReactionParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            reaction_id: params.get("reaction-id").parse()?,
            thread: params.try_into()?,
        })
    }
}

impl WithReactedToId for ThreadParams {
    /// Returns `reply_id` when `Some` and `thread_id` when `None`.
    fn reacted_to_id(&self) -> &Id {
        self.reply_id().unwrap_or(self.thread_id())
    }
}
