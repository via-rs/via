use std::process::ExitCode;
use via::ws::{self, Channel};
use via::{Error, Finalize, Next, Request, Response, Server};

async fn echo(request: Request, _: Next) -> via::Result {
    request.finalize(Response::build())
}

async fn echo_ws(mut channel: Channel, _: ws::Request) -> ws::Result {
    loop {
        let Some(message) = channel.recv().await else {
            return Ok(());
        };

        if message.is_close() {
            eprintln!("info: close requested by client");
            return Ok(());
        }

        if message.is_binary() || message.is_text() {
            channel.send(message).await?;
        } else if cfg!(debug_assertions) {
            eprintln!("warn: ignoring message {:?}", message);
        }
    }
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let mut app = via::app(());

    app.route("/echo").to(via::post(echo).get(via::ws(echo_ws)));

    Server::new(app).listen(("127.0.0.1", 8080)).await
}
