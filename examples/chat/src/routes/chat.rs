use via::Error;
use via::ws::{self, Channel, ResultExt};

type Request = ws::Request<crate::Unicorn>;

pub async fn chat(mut channel: Channel, _request: Request) -> ws::Result {
    while let Some(_message) = channel.recv().await {
        Err(Error::new("not yet implemented")).or_break()?
    }

    Ok(())
}
