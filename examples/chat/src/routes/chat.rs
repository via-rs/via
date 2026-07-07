use serde::Deserialize;
use via::error::Propagate;
use via::ws::{self, Channel};

use crate::Unicorn;
use crate::util::Session;

#[derive(Deserialize)]
struct Message<'a> {
    body: &'a str,
}

macro_rules! log {
    ($($args:tt)*) => {
        // Placeholder for tracing...
        #[cfg(debug_assertions)]
        eprintln!($($args)*);
    };
}

pub async fn chat(mut channel: Channel, request: ws::Request<Unicorn>) -> ws::Result {
    log!("  info(examples/chat): setup ws recv loop.");

    // An active session is required to keep the web socket open.
    let Some(_session) = request.session().copied() else {
        return via::err!(401, "unauthorized.").or_break();
    };

    while let Some(next) = channel.recv().await {
        // End the session if the next message is a close message or the active
        // user no longer has an account.
        if next.is_close()
        /* || !session.verify(request.app().database()).await */
        {
            break;
        }

        // Deserialize a `Message` from `next` if it contains data.
        let message = if next.is_binary() || next.is_text() {
            // We require our users to send us valid UTF-8 even if the message
            // payload contains binary data. Verify that payload is valid UTF-8
            // or restart the session.

            #[cfg(feature = "tokio-tungstenite")]
            let text = next.to_text().or_continue()?;

            #[cfg(feature = "tokio-websockets")]
            let text = str::from_utf8(next.as_payload()).or_continue()?;

            // In this example, we want to keep the number of allocations as
            // low as possible. Therefore, we use `serde_json` directly...
            serde_json::from_str::<Message>(text).or_continue()?
        } else {
            log!("  warn(examples/chat): ignoring message {:?}", next);
            continue; // Other message types are handled internally. Continue.
        };

        eprintln!("  info(examples/chat): {}", &message.body);
    }

    log!("    info(examples/chat): ws session ended.");

    Ok(())
}
