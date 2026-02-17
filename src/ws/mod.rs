#[cfg(all(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
compile_error!("Features \"tokio-tungstenite\" and \"tokio-websockets\" are mutually exclusive.");

mod channel;
mod error;
mod request;
mod upgrade;

pub use channel::*;
pub use error::{Result, ResultExt};
pub use request::Request;
pub use upgrade::Ws;

/// Upgrade the connection to a web socket.
///
/// # Example
///
/// ```no_run
/// use std::process::ExitCode;
/// use via::ws::{self, Channel, Message};
/// use via::{Error, Server};
///
/// async fn echo(mut channel: Channel, _: ws::Request<()>) -> ws::Result {
///     while let Some(message) = channel.recv().await {
///         match message {
///             forward @ (Message::Binary(_) | Message::Text(_)) => {
///                 channel.send(forward).await?;
///             }
///             ignore => {
///                 if cfg!(debug_assertions) {
///                     println!("{:?}", ignore);
///                 }
///             }
///         }
///     }
///
///     Ok(())
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<ExitCode, Error> {
///     let mut app = via::app(());
///
///     // GET /echo ~> web socket upgrade.
///     app.route("/echo").to(via::ws(echo));
///
///     Server::new(app).listen(("127.0.0.1", 8080)).await
/// }
///```
///
pub fn ws<T, App, Await>(listener: T) -> Ws<T>
where
    T: Fn(Channel, Request<App>) -> Await,
    Await: Future<Output = Result> + Send,
{
    Ws::new(listener)
}
