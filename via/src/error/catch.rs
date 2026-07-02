use std::ops::ControlFlow;

use super::Error;

/// Indicates how a recoverable error should affect contextual control flow.
///
/// `Catch` is returned from fallible operations whose errors may be handled in
/// more than one way. Rather than deciding globally whether an error is fatal,
/// `Catch` lets the caller describe whether the error should break out of the
/// current context or continue from the next recoverable boundary.
///
/// The meaning of [`ControlFlow::Break`] and [`ControlFlow::Continue`] depends
/// on where the `Catch` is handled:
///
/// - In [`before`] middleware, `Break` rejects the request while `Continue`
///   skips the wrapped middleware and resumes execution with [`Next`].
///
/// - In a WebSocket listener passed to [`ws`], `Break` closes the connection
///   while `Continue` keeps the connection open and restarts the listener.
///
/// - In other adapters, `Break` and `Continue` may map to whatever boundary the
///   adapter defines as fatal or recoverable.
///
/// See [`Propagate`] for convenience methods that convert common result-like
/// data structures into a result with an error type of `Catch`.
///
/// [`Next`]: crate::Next
/// [`before`]: fn@crate::before
/// [`ws`]: fn@crate::ws
pub type Catch = ControlFlow<Error, Error>;

/// Convert result-like data structures into contextual control flow.
///
/// `Propagate` provides convenience methods for indicating whether an error
/// is considered fatal or recoverable.
///
/// The context in which a [`Catch`] occurs determines the significance of each
/// discriminant in [`ControlFlow`].
///
/// # Example
///
/// When returned from a request decorator in [`before`] middleware, the
/// `Continue` variant skips the wrapped middleware and resumes execution with
/// [`Next`], while the `Break` variant immediately rejects the request with
/// the contained error.
///
/// ```
/// use via::error::{Catch, Propagate};
/// use via::{Request, err};
/// #
/// # #[derive(Clone, Copy, PartialEq)]
/// # struct Identity([u8; 16]);
/// #
/// # impl std::str::FromStr for Identity {
/// #     type Err = via::Error;
/// #
/// #     fn from_str(input: &str) -> Result<Self, Self::Err> {
/// #          todo!()
/// #     }
/// # }
///
/// pub fn restore(request: &mut Request) -> Result<(), Catch> {
///     let token = request
///         .cookies()
///         .get("via-session")
///         .ok_or_else(|| err!(401, "unauthorized."))
///         .and_then(|cookie| cookie.value().parse::<Identity>())
///         .or_continue()?;
///     //   ^^^^^^^^^^^
///     //
///     // Continue to the next middleware.
///     // The user may be trying to create an account or login.
///     //
///     // Insert the identity token into the request extensions.
///     request.extensions_mut().insert(token);
///     //
///     // Request decorated successfully.
///     Ok(())
/// }
/// ```
///
/// When returned from a web socket listener passed to [`ws`], the `Continue`
/// discriminant keeps the web socket connection open and restarts the listener
/// where the `Break` variant immediately closes the connection.
///
/// ```
/// use std::ops::ControlFlow;
///
/// use via::error::Propagate;
/// use via::ws::{self, Channel, Request};
/// #
/// # #[derive(Clone, Copy)]
/// # pub struct Identity([u8; 16]);
/// #
/// # pub trait Session {
/// #     fn session(&self) -> Option<&Identity>;
/// # }
/// #
/// # impl Session for Request {
/// #     fn session(&self) -> Option<&Identity> { todo!() }
/// # }
///
/// pub async fn echo(mut channel: Channel, request: Request) -> ws::Result {
///     let Some(_me) = request.session().copied() else {
///         return via::err!(401, "unauthorized.").or_break();
///         //                                     ^^^^^^^^
///         // If the request is not authenticated, close the connection.
///     };
///
///     while let Some(message) = channel.recv().await {
///         if message.is_binary() || message.is_text() {
///             let text = message.to_text().or_continue()?;
///             //                           ^^^^^^^^^^^
///             // Invalid UTF-8 rejects this frame and restarts the receive loop.
///             //
///             // Valid UTF-8 is echoed back to the client.
///             channel.send(text).await?;
///             //                      ^
///             // Send errors unconditionally close the connection as they
///             // imply a dropped receiver.
///         } else if message.is_close() {
///             break; // A close opcode gracefully terminates the listener.
///         }
///     }
///
///     Ok(())
/// }
/// ```
///
/// [`Next`]: crate::Next
/// [`before`]: fn@crate::before
/// [`ws`]: fn@crate::ws
pub trait Propagate {
    /// The successful output of the operation.
    type Output;

    /// Marks a fallible operation as fatal.
    ///
    /// The exact behavior depends on where the resulting [`Catch`] is handled.
    fn or_break(self) -> Result<Self::Output, Catch>;

    /// Marks a fallible operation as recoverable.
    ///
    /// The exact behavior depends on where the resulting [`Catch`] is handled.
    fn or_continue(self) -> Result<Self::Output, Catch>;
}

impl Propagate for Error {
    type Output = ();

    fn or_break(self) -> Result<Self::Output, Catch> {
        Err(ControlFlow::Break(self))
    }

    fn or_continue(self) -> Result<Self::Output, Catch> {
        Err(ControlFlow::Continue(self))
    }
}

impl<T, E> Propagate for Result<T, E>
where
    Error: From<E>,
{
    type Output = T;

    #[inline]
    fn or_break(self) -> Result<Self::Output, Catch> {
        self.map_err(|error| ControlFlow::Break(error.into()))
    }

    #[inline]
    fn or_continue(self) -> Result<Self::Output, Catch> {
        self.map_err(|error| ControlFlow::Continue(error.into()))
    }
}
