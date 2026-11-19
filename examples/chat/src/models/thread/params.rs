use diesel::{
    BoolExpressionMethods, BoxableExpression, ExpressionMethods, NullableExpressionMethods,
};
use diesel::{pg::Pg, sql_types};
use serde::Deserialize;
use via::request::PathParams;
use via::{Error, ResultExt};

use super::JoinUsers;

use crate::Request;
use crate::models::subscription::SubscriptionKeys;
use crate::models::{ChannelSubscription, Subscription};
use crate::routes::channels::Subscriber;
use crate::schema::threads;
use crate::util::{BoxedFilterExpr, Id};

pub struct ThreadCollectionContext {
    thread_id: Option<Id>,
    keys: SubscriptionKeys,
}

pub struct ThreadMemberContext {
    params: ThreadParams,
    keys: SubscriptionKeys,
}

#[derive(Debug)]
struct ThreadParams {
    thread_id: Id,
    reply_id: Option<Id>,
}

impl ThreadCollectionContext {
    pub fn new(keys: SubscriptionKeys, thread_id: Option<Id>) -> Self {
        Self { keys, thread_id }
    }

    pub fn channel_id(&self) -> &Id {
        &self.keys.channel_id
    }

    pub fn thread_id(&self) -> Option<&Id> {
        self.thread_id.as_ref()
    }

    pub fn user_id(&self) -> &Id {
        &self.keys.user_id
    }

    pub fn to_where_clause(&self) -> BoxedFilterExpr<'_, JoinUsers> {
        if let Some(thread_id) = self.thread_id() {
            let expr = threads::channel_id
                .eq(self.channel_id())
                .and(threads::thread_id.eq(thread_id));

            Box::new(expr)
        } else {
            let expr = threads::channel_id
                .eq(self.channel_id())
                .and(threads::thread_id.is_null())
                .nullable();

            Box::new(expr)
        }
    }
}

impl ThreadMemberContext {
    pub fn channel_id(&self) -> &Id {
        &self.keys.channel_id
    }

    pub fn thread_id(&self) -> &Id {
        &self.params.thread_id
    }

    pub fn reply_id(&self) -> Option<&Id> {
        self.params.reply_id.as_ref()
    }

    pub fn user_id(&self) -> &Id {
        &self.keys.user_id
    }

    pub fn to_delete_where(&self) -> BoxedFilterExpr<'_, threads::table> {
        if let Some(reply_id) = self.reply_id() {
            Box::new(
                threads::id
                    .eq(reply_id)
                    .and(threads::user_id.eq(self.user_id()))
                    .and(threads::thread_id.eq(self.thread_id())),
            )
        } else {
            Box::new(
                threads::id
                    .eq(self.thread_id())
                    .and(threads::user_id.eq(self.user_id()))
                    .and(threads::thread_id.is_null())
                    .nullable(),
            )
        }
    }
}

impl<'a> TryFrom<&'a Request> for ThreadCollectionContext {
    type Error = Error;

    fn try_from(request: &'a Request) -> Result<Self, Self::Error> {
        Ok(Self {
            // Load the channel subscription that we resolved during authorization.
            keys: request
                .channel()
                .map(|channel| channel.keys())
                .or_forbidden()?,

            // Parse the optional thread-id parameter from the request URI Path.
            thread_id: request.param("thread-id").ok_and_then(|id| id.parse())?,
        })
    }
}

impl<'a> TryFrom<&'a Request> for ThreadMemberContext {
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

impl<'a> TryFrom<PathParams<'a>> for ThreadParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            thread_id: params.get("thread-id").parse()?,
            reply_id: params.get("reply-id").ok_and_then(|id| id.parse())?,
        })
    }
}
