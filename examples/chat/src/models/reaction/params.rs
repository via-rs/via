use delegate::delegate;
use via::{Error, request::PathParams};

use crate::Request;
use crate::models::subscription::SubscriptionKeys;
use crate::models::thread::ThreadParams;
use crate::routes::channels::Subscriber;
use crate::util::Id;

pub struct ReactionCollectionContext {
    params: ThreadParams,
    keys: SubscriptionKeys,
}

pub struct ReactionMemberContext {
    params: ReactionParams,
    keys: SubscriptionKeys,
}

#[derive(Debug)]
struct ReactionParams {
    thread: ThreadParams,
    reaction_id: Id,
}

impl ReactionCollectionContext {
    delegate! {
        to self.keys() {
            pub fn channel_id(&self) -> &Id;
            pub fn user_id(&self) -> &Id;
        }
    }

    pub fn thread_id(&self) -> &Id {
        self.params.reply_id().unwrap_or(self.params.thread_id())
    }

    fn keys(&self) -> &SubscriptionKeys {
        &self.keys
    }

    fn params(&self) -> &ThreadParams {
        &self.params
    }
}

impl ReactionMemberContext {
    delegate! {
        to self.keys() {
            pub fn channel_id(&self) -> &Id;
            pub fn user_id(&self) -> &Id;
        }
    }

    pub fn reaction_id(&self) -> &Id {
        &self.params.reaction_id
    }

    pub fn thread_id(&self) -> &Id {
        self.thread()
            .reply_id()
            .unwrap_or(self.thread().thread_id())
    }

    // pub fn to_delete_where(&self) -> BoxedFilterExpr<'_, threads::table> {
    //     if let Some(reply_id) = self.reply_id() {
    //         Box::new(
    //             threads::id
    //                 .eq(reply_id)
    //                 .and(threads::user_id.eq(self.user_id()))
    //                 .and(threads::thread_id.eq(self.thread_id()))
    //                 .and(threads::channel_id.eq(self.channel_id())),
    //         )
    //     } else {
    //         Box::new(
    //             threads::id
    //                 .eq(self.thread_id())
    //                 .and(threads::user_id.eq(self.user_id()))
    //                 .and(threads::thread_id.is_null())
    //                 .and(threads::channel_id.eq(self.channel_id()))
    //                 .nullable(),
    //         )
    //     }
    // }

    fn keys(&self) -> &SubscriptionKeys {
        &self.keys
    }

    fn thread(&self) -> &ThreadParams {
        &self.params.thread
    }
}

impl<'a> TryFrom<&'a Request> for ReactionCollectionContext {
    type Error = Error;

    fn try_from(request: &'a Request) -> Result<Self, Self::Error> {
        Ok(Self {
            // Load the authorized channel subscription keys.
            keys: request.channel_keys()?,

            // Extract the thread params from the request URI path.
            params: request.params()?,
        })
    }
}

impl<'a> TryFrom<&'a Request> for ReactionMemberContext {
    type Error = Error;

    fn try_from(request: &'a Request) -> Result<Self, Self::Error> {
        Ok(Self {
            // Load the authorized channel subscription keys.
            keys: request.channel_keys()?,

            // Extract the thread params from the request URI path.
            params: request.params()?,
        })
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
