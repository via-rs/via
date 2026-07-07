via::resource!(app = Unicorn);

use serde::Serialize;
use via::Error;
use via::request::PathParams;

use super::channels::ChannelMemberParams;
use crate::database::Id;
use crate::{Next, Request, Unicorn};

#[derive(Clone, Debug, Serialize)]
pub struct ReactionCollectionParams {
    channel: ChannelMemberParams,
    reply_id: Option<Id>,
    thread_id: Id,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReactionMemberParams {
    channel: ChannelMemberParams,
    reply_id: Option<Id>,
    thread_id: Id,
    reaction_id: Id,
}

async fn index(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

async fn create(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

async fn show(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

async fn update(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

async fn destroy(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

impl<'a> TryFrom<PathParams<'a>> for ReactionCollectionParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            channel: params.try_into()?,
            reply_id: params.get("reply-id").parse().ok(),
            thread_id: params.get("thread-id").parse()?,
        })
    }
}

impl<'a> TryFrom<PathParams<'a>> for ReactionMemberParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            channel: params.try_into()?,
            reply_id: params.get("reply-id").parse().ok(),
            thread_id: params.get("thread-id").parse()?,
            reaction_id: params.get("reaction-id").parse()?,
        })
    }
}
