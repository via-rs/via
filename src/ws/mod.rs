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
/// ```
/// use via::ws::{self, Channel, Message, Request};
/// use via::{Error, Payload};
///
/// async fn echo(mut channel: Channel, _: Request) -> ws::Result {
///     loop {
///         let Some(message) = channel.recv().await else {
///             break Ok(());
///         };
///
///         if let Message::Close(close) = &message {
///             close.as_ref().inspect(|(code, reason)| {
///                 eprintln!("{:?}: {:?}", code, reason);
///             });
///
///             break Ok(());
///         }
///
///         channel.send(message).await?;
///     }
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Error> {
///     let mut app = via::app(());
///
///     // GET /echo ~> web socket upgrade.
///     app.route("/echo").to(ws::upgrade(echo));
///
///     Ok(())
/// }
///```
///
pub fn upgrade<App, F, R>(upgraded: F) -> Ws<F>
where
    F: Fn(Channel, Request<App>) -> R + Send + Sync + 'static,
    R: Future<Output = Result> + Send,
{
    Ws::new(upgraded)
}
