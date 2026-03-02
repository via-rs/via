use std::process::ExitCode;
use via::ws::{self, Channel};
use via::{Error, Finalize, Next, Request, Response, Server};

async fn echo(request: Request, _: Next) -> via::Result {
    request.finalize(Response::build())
}

async fn relay(mut channel: Channel, _: ws::Request) -> ws::Result {
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

    app.route("/echo").to(via::get(via::ws(relay)).post(echo));

    Server::new(app).listen(("127.0.0.1", 8080)).await
}
