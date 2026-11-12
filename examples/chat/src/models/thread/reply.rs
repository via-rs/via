use via::{Error, request::PathParams};

use crate::util::Id;

#[derive(Clone, Debug)]
pub struct ReplyParams {
    pub thread_id: Id,
    pub reply_id: Id,
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
