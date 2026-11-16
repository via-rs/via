use serde::Deserialize;
use via::{Error, request::PathParams};

use crate::models::Subscription;
use crate::util::Id;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThreadParams {
    pub(super) channel_id: Id,
    #[serde(rename = "threadId")]
    pub(super) parent_id: Option<Id>,
    pub(super) user_id: Option<Id>,
}

#[derive(Clone, Debug)]
pub struct ReplyParams {
    pub thread_id: Id,
    pub reply_id: Id,
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
