use http::StatusCode;
use serde::Serialize;
use via::request::PathParams;
use via::{Error, Payload, Response, deny};

use super::channels::ChannelMemberParams;
use crate::database::Id;
use crate::database::models::{NewReaction, Reaction};
use crate::util::{Body, Session};
use crate::{Next, Request};

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

pub async fn index(request: Request, _: Next) -> via::Result {
    let params = request.params::<ReactionCollectionParams>()?;

    Response::build().json(&Body::new(params))
}

pub async fn create(request: Request, _: Next) -> via::Result {
    let _user = request.user().await?;
    let params = request.params::<ReactionCollectionParams>()?;
    let (future, _) = request.into_future();
    let new_reaction: Body<NewReaction> = future.await?.json()?;

    if cfg!(debug_assertions) {
        println!("  params = {:#?}", &params);
        println!("  body = {:#?}", &new_reaction);
    }

    Response::build()
        .status(StatusCode::CREATED)
        .json(&Body::new(Reaction::new(new_reaction.data)?))
}

pub async fn show(request: Request, _: Next) -> via::Result {
    let params = request.params::<ReactionMemberParams>()?;

    if cfg!(debug_assertions) {
        println!("  params = {:#?}", &params);
    }

    Response::build().json(&Body::new(params))
}

pub async fn update(_: Request, _: Next) -> via::Result {
    deny!(500, "todo!")
}

pub async fn destroy(_: Request, _: Next) -> via::Result {
    deny!(500, "todo!")
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
