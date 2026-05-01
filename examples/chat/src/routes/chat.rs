use serde::Deserialize;
use via::Payload;
use via::ws::{self, Channel, ResultExt};

use crate::util::Session;

type Request = ws::Request<crate::Unicorn>;

#[derive(Deserialize)]
struct Message {
    body: String,
}

pub async fn chat(mut channel: Channel, request: Request) -> ws::Result {
    if cfg!(debug_assertions) {
        eprintln!("  info(examples/chat): setup ws recv loop.");
    }

    let Some(mut session) = request.session().cloned() else {
        return via::err!(401, "unauthorized.").or_close();
    };

    while let Some(next) = channel.recv().await {
        // Confirm that the user still exists before we proceed.
        if session.is_expired() {
            if session.user(request.app().database()).await.is_err() {
                break;
            }

            session = session.refresh();
        }

        // Stop receiving messages if the client ends the session.
        if next.is_close() {
            break;
        }

        if next.is_text() {
            // The impl of Payload::json for ws::Message does not allocate.
            // However, fields of the deserialized type can allocate.
            let message = next.json::<Message>().or_reconnect()?;
            //                                   ^^^^^^^^^^^^
            // If an error occurs while deserializing the message due to
            // malformed user input, restart the session rather than ending it.
            println!("      info(examples/chat): {}", &message.body);
        } else if cfg!(debug_assertions) {
            eprintln!("      warn(examples/chat): ignoring message {:?}", next);
        }

        // Yield to the runtime to uphold the web socket reactor contract.
        //
        // This isn't necessary if you await any other future that is not
        // immediately ready.
        //
        // For example, sending a reply, acquiring a database connection, or
        // sending a command to a database unconditionally is enough to
        // guarantee progress when we're not waiting for I/O.
        tokio::task::yield_now().await;
    }

    if cfg!(debug_assertions) {
        eprintln!("      info(examples/chat): ws session ended.");
    }

    Ok(())
}
