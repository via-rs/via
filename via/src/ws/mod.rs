//! Support bidirectional communication over HTTP with WebSockets.

#[cfg(all(feature = "aws-lc-rs", feature = "ring"))]
compile_error!("features \"aws-lc-rs\" and \"ring\" are mutually exclusive.");

#[cfg(not(any(feature = "aws-lc-rs", feature = "ring")))]
compile_error!("either \"aws-lc-rs\" or \"ring\" must be enabled to use the ws module.");

#[cfg(all(feature = "tokio-tungstenite", feature = "tokio-websockets"))]
compile_error!("features \"tokio-tungstenite\" and \"tokio-websockets\" are mutually exclusive.");

mod channel;
mod error;
mod request;
mod run;
mod upgrade;
mod util;

pub use channel::*;
pub use error::Result;
pub use request::Request;
pub use upgrade::Ws;

/// Upgrade the connection to a web socket.
///
/// # Example
///
/// ```no_run
/// use std::process::ExitCode;
/// use via::ws::{self, Channel, Message};
/// use via::{Error, Router, Server};
///
/// async fn echo(mut channel: Channel, _: ws::Request) -> ws::Result {
///     while let Some(message) = channel.recv().await {
///         if message.is_close() {
///             println!("info: close requested by client");
///             break;
///         }
///
///         if message.is_binary() || message.is_text() {
///             channel.send(message).await?;
///         } else if cfg!(debug_assertions) {
///             println!("warn: ignoring message {:?}", message);
///         }
///     }
///
///     Ok(())
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<ExitCode, Error> {
///     let router = Router::new(|home| {
///         // Start defining the descendats of "/".
///         let mut path = home.prefix();
///
///         // GET /echo ~> web socket upgrade.
///         path.route("/echo", via::get(via::ws(echo)).or_deny());
///     });
///
///     // Start listening at http://localhost:8080/ for incoming requests.
///     Server::new(router, ()).listen(("127.0.0.1", 8080)).await
/// }
///```
pub fn ws<T, App, Await>(listener: T) -> Ws<T>
where
    T: Fn(Channel, Request<App>) -> Await,
    Await: Future<Output = Result> + Send,
{
    Ws::new(listener)
}
