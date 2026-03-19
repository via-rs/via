use via::request::params::PathParams;
use via::{Error, Next, Request, Response, raise};

use super::channels::ChannelMemberParams;

#[derive(Debug)]
pub struct ReactionCollectionParams {
    channel: ChannelMemberParams,
    reply_id: Option<u64>,
    thread_id: u64,
}

pub async fn index(request: Request, _: Next) -> via::Result {
    let params = request.params::<ReactionCollectionParams>()?;

    Response::build().text(format!("{:#?}", params))
}

pub async fn create(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn show(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn update(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn destroy(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

impl<'a> TryFrom<PathParams<'a>> for ReactionCollectionParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        let thread_id = params.get("thread-id").parse()?;
        let reply_id = params.get("reply-id").ok().and_then(|optional| {
            if let Some(value) = optional.as_deref() {
                Ok(Some(value.parse()?))
            } else {
                Ok(None)
            }
        })?;

        Ok(Self {
            channel: params.clone().try_into()?,
            reply_id,
            thread_id,
        })
    }
}
