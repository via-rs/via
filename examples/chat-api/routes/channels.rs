use via::request::params::PathParams;
use via::{Error, Next, Request, raise};

#[derive(Debug)]
pub struct ChannelMemberParams {
    pub channel_id: u64,
    pub org_id: u64,
}

pub async fn authorization(request: Request, next: Next) -> via::Result {
    next.call(request).await
}

pub async fn index(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
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

impl<'a> TryFrom<PathParams<'a>> for ChannelMemberParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            channel_id: params.get("channel-id").parse()?,
            org_id: params.get("org-id").parse()?,
        })
    }
}
