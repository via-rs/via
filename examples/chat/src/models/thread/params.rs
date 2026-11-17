use diesel::pg::Pg;
use diesel::{BoolExpressionMethods, BoxableExpression, NullableExpressionMethods, sql_types};
use serde::Deserialize;
use via::ResultExt;
use via::{Error, request::PathParams};

use super::{by_id, by_thread};
use crate::models::Subscription;
use crate::models::thread::{by_user, is_thread, thread_is_null, user_is_null};
use crate::schema::threads;
use crate::util::Id;

pub type ThreadParamsExpr =
    Box<dyn BoxableExpression<threads::table, Pg, SqlType = sql_types::Nullable<sql_types::Bool>>>;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThreadParams {
    pub(super) channel_id: Id,
    #[serde(rename = "threadId")]
    pub(super) parent_id: Option<Id>,
    pub(super) user_id: Option<Id>,
}

pub struct UpdateThreadScope {
    pub(super) author_id: Id,
    pub(super) update_target: Result<Id, ReplyParams>,
}

#[derive(Clone, Debug)]
pub struct ReplyParams {
    pub thread_id: Id,
    pub reply_id: Id,
}

#[derive(Clone, Debug)]
pub struct ThreadParams {
    thread_id: Id,
    reply_id: Option<Id>,
    pub user_id: Option<Id>,
}

impl CreateThreadParams {
    #[inline]
    pub fn with_parent_id(&mut self, id: Option<Id>) {
        self.parent_id = id;
    }
}

impl TryFrom<&'_ Subscription> for CreateThreadParams {
    type Error = Error;

    fn try_from(subscription: &Subscription) -> Result<Self, Self::Error> {
        Ok(Self {
            channel_id: subscription.channel_id(),
            parent_id: None,
            user_id: Some(subscription.user_id()),
        })
    }
}

impl<'a> TryFrom<PathParams<'a>> for ReplyParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            thread_id: params.get("thread-id").parse()?,
            reply_id: params.get("reply-id").parse()?,
        })
    }
}

impl ThreadParams {
    #[rustfmt::skip]
    pub fn to_boxed_expr(&self) -> ThreadParamsExpr {
        // Expressions should be considered frozen once they are boxed.
        //
        // This limits the amount of dynamic dispatch calls required to
        // generate the SQL query.
        match self {
            Self { thread_id, reply_id: Some(id), user_id: Some(user_id) } => {
                Box::new(by_id(*id).and(by_thread(*thread_id)).and(by_user(*user_id)))
            }
            Self { thread_id, reply_id: Some(id), .. } => {
                Box::new(by_id(*id).and(by_thread(*thread_id)).nullable())
            }
            Self { thread_id: id, user_id: Some(user_id), .. } => {
                Box::new(by_id(*id).and(by_user(*user_id)).nullable())
            }
            Self { thread_id: id, .. } => {
                Box::new(by_id(*id).nullable())
            }
        }
    }

    #[inline]
    pub fn with_user_id(&mut self, id: Id) {
        self.user_id = Some(id);
    }
}

impl<'a> TryFrom<PathParams<'a>> for ThreadParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            thread_id: params.get("thread-id").parse()?,
            reply_id: params.get("reply-id").ok_and_then(|id| id.parse())?,
            user_id: None,
        })
    }
}
