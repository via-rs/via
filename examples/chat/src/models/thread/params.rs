use diesel::pg::Pg;
use diesel::{BoolExpressionMethods, BoxableExpression, NullableExpressionMethods, sql_types};
use serde::Deserialize;
use via::request::PathParams;
use via::{Error, ResultExt};

use crate::models::Subscription;
use crate::schema::threads;
use crate::util::Id;

pub type ThreadParamsExpr<T> =
    Box<dyn BoxableExpression<T, Pg, SqlType = sql_types::Nullable<sql_types::Bool>>>;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThreadParams {
    #[serde(rename = "threadId")]
    pub parent_id: Option<Id>,

    pub(super) channel_id: Id,
    pub(super) user_id: Option<Id>,
}

#[derive(Debug)]
pub struct ThreadParams {
    pub thread_id: Id,
    pub reply_id: Option<Id>,
    pub user_id: Option<Id>,
}

pub(super) struct UpdateThreadParams<'a> {
    pub(super) primary_key: &'a Id,
    pub(super) thread_id: Option<&'a Id>,
    pub(super) user_id: &'a Id,
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

impl ThreadParams {
    pub fn id(&self) -> &Id {
        match self {
            Self {
                thread_id: id,
                reply_id: None,
                ..
            }
            | Self {
                reply_id: Some(id), ..
            } => id,
        }
    }

    pub(super) fn try_as_update(&self) -> via::Result<UpdateThreadParams<'_>> {
        match self {
            Self {
                thread_id: primary_key,
                reply_id: None,
                user_id: Some(user_id),
            } => Ok(UpdateThreadParams {
                primary_key,
                thread_id: None,
                user_id,
            }),
            Self {
                thread_id,
                reply_id: Some(primary_key),
                user_id: Some(user_id),
            } => Ok(UpdateThreadParams {
                primary_key,
                thread_id: Some(thread_id),
                user_id,
            }),
            _ => {
                via::deny!(403, "Forbidden");
            }
        }
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
