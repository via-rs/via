#[cfg(all(feature = "aws-lc-rs", feature = "ring"))]
compile_error!("Features \"aws-lc-rs\" and \"ring\" are mutually exclusive.");

#[cfg(not(any(feature = "aws-lc-rs", feature = "ring")))]
compile_error!("A crypto backend must be enabled: either \"aws-lc-rs\" or \"ring\".");

mod channel;
mod error;
mod upgrade;

pub use channel::{Channel, CloseFrame, Message, Utf8Bytes};
pub use error::{Result, ResultExt};
pub use upgrade::{Request, Ws};

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
