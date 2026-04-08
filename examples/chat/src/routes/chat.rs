use tokio::task::yield_now;
use via::ws::{self, Channel, Message, ResultExt};

type Request = ws::Request<crate::Unicorn>;

pub async fn chat(mut channel: Channel, _request: Request) -> ws::Result {
    let fail = || via::raise!(message = "every other message succeeds");
    let mut n = 0;

    while let Some(message) = channel.recv().await {
        n += 1;

        if n % 2 == 0 {
            fail().or_reconnect()?;
        }

        if let Message::Text(text) = &message {
            println!("  info(chat): {}", text.as_str());
        }

        // Await a future that yields to the runtime in order to uphold of the
        // contract of the ws reactor.
        yield_now().await;
    }

    Ok(())
}
