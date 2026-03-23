use std::process::ExitCode;
use via::{Error, Finalize, Next, Request, Response, Server};

async fn echo(request: Request, _: Next) -> via::Result {
    request.finalize(Response::build())
}

#[cfg(not(all(
    any(feature = "aws-lc-rs", feature = "ring"),
    any(feature = "tokio-tungstenite", feature = "tokio-websockets")
)))]
async fn relay(request: Request, next: Next) -> via::Result {
    next.call(request).await
}

#[cfg(all(
    any(feature = "aws-lc-rs", feature = "ring"),
    any(feature = "tokio-tungstenite", feature = "tokio-websockets")
))]
async fn relay(mut channel: via::ws::Channel, _: via::ws::Request) -> via::ws::Result {
    while let Some(message) = channel.recv().await {
        if message.is_close() {
            eprintln!("info: close requested by client");
            break;
        }

        if message.is_binary() || message.is_text() {
            channel.send(message).await?;
        } else if cfg!(debug_assertions) {
            eprintln!("warn: ignoring message {:?}", message);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let mut app = via::app(());

    #[cfg(any(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
    let relay = via::ws(relay);

    app.route("/echo").to(via::post(echo).get(relay));

    Server::new(app).listen(("127.0.0.1", 8080)).await
}
