via::resource!(app = Unicorn);

use via::Error;
use via::request::PathParams;

use crate::util::Id;
use crate::{Next, Request, Unicorn};

#[derive(Debug)]
pub struct ReactionParams {
    reaction_id: Id,
    thread_id: Id,
    reply_id: Option<Id>,
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

impl<'a> TryFrom<PathParams<'a>> for ReactionParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            reaction_id: params.get("reaction-id").parse()?,
            thread_id: params.get("thread-id").parse()?,
            reply_id: params.get("reply-id").ok_and_then(str::parse)?,
        })
    }
}
