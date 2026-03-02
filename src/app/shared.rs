use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;
use std::sync::Arc;

/// A thread-safe, reference-counting pointer to the application.
///
/// An application is a user-defined struct that bundles together singleton
/// resources whose lifetime matches that of the process in which it is created.
///
/// `Shared` wraps an application and provides per-request ownership of the
/// container. This allows resources to flow through async code without creating
/// dangling borrows or introducing implicit lifetimes.
///
/// Cloning a `Shared<App>` is inexpensive: it performs an atomic increment when
/// cloned and an atomic decrement when dropped. When a client request is
/// received, the `Shared` wrapper is cloned and ownership of the clone is
/// transferred to the request.
///
/// # Safe Access
///
/// Async functions are compiled into state machines that may be suspended across
/// `.await` points. Any borrow that outlives the data it references becomes
/// invalid when the future is resumed, violating Rust’s safety guarantees.
///
/// For many ["safe" (read-only)](http::Method::is_safe) requests, the application
/// can be borrowed directly from the request without cloning or taking ownership
/// of the underlying `Shared<App>`.
///
/// ### Example
///
/// ```
/// use serde::{Deserialize, Serialize};
/// use tokio::io::{self, AsyncWriteExt, Sink};
/// use tokio::sync::{Mutex, RwLock};
/// use uuid::Uuid;
/// use via::{Next, Payload, Request, Response};
///
/// /// An imaginary analytics service.
/// struct Telemetry(Mutex<Sink>);
///
/// /// Our billion dollar application.
/// struct Unicorn {
///     telemetry: Telemetry,
///     users: RwLock<Vec<User>>,
/// }
///
/// #[derive(Clone, Deserialize)]
/// struct NewUser {
///     email: String,
///     username: String,
/// }
///
/// #[derive(Clone, Serialize)]
/// struct User {
///     id: Uuid,
///     email: String,
///     username: String,
/// }
///
/// impl Telemetry {
///     async fn report(&self, message: String) -> io::Result<()> {
///         let mut guard = self.0.lock().await;
///
///         guard.write_all(message.as_bytes()).await?;
///         guard.flush().await
///     }
/// }
///
/// impl From<NewUser> for User {
///     fn from(new_user: NewUser) -> Self {
///         Self {
///             id: Uuid::new_v4(),
///             email: new_user.email,
///             username: new_user.username,
///         }
///     }
/// }
///
/// async fn find_user(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
///     // Parse a UUID from the :user-id parameter in the request URI.
///     let id = request.param("user-id").parse::<Uuid>()?;
///
///     let user = {
///         // Acquire a read lock on the list of users.
///         let guard = request.app().users.read().await;
///
///         // Find the user with id = :id and clone it to keep contention low.
///         guard.iter().find(|user| id == user.id).cloned()
///     };
///
///     Response::build()
///         .status(user.is_some().then_some(200).unwrap_or(404))
///         .json(&user)
/// }
/// ```
///
/// ## Handling Mutations
///
/// For non-idempotent HTTP requests (e.g., DELETE, PATCH, POST), it is often
/// necessary to consume the request in order to read the message body.
///
/// In these cases, ownership of the `Shared<App>` is returned to the caller.
/// This commonly occurs when a mutation requires acquiring a database
/// connection or persisting state derived from the request body.
///
/// This access pattern is safe, but any clone of `Shared<App>` that escapes the
/// request future extends the lifetime of the application container and should
/// be treated as an intentional design choice.
///
/// ### Example
///
/// ```
/// # use serde::{Deserialize, Serialize};
/// # use tokio::io::Sink;
/// # use tokio::sync::{Mutex, RwLock};
/// # use uuid::Uuid;
/// # use via::{Next, Payload, Request, Response};
/// #
/// # /// An imaginary analytics service.
/// # struct Telemetry(Mutex<Sink>);
/// #
/// # /// Our billion dollar application.
/// # struct Unicorn {
/// #     telemetry: Telemetry,
/// #     users: RwLock<Vec<User>>,
/// # }
/// #
/// # #[derive(Clone, Deserialize)]
/// # struct NewUser {
/// #     email: String,
/// #     username: String,
/// # }
/// #
/// # #[derive(Clone, Serialize)]
/// # struct User {
/// #     id: Uuid,
/// #     email: String,
/// #     username: String,
/// # }
/// #
/// # impl From<NewUser> for User {
/// #     fn from(new_user: NewUser) -> Self {
/// #         Self {
/// #             id: Uuid::new_v4(),
/// #             email: new_user.email,
/// #             username: new_user.username,
/// #         }
/// #     }
/// # }
/// #
/// async fn create_user(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
///     let (future, app) = request.into_future();
///     //           ^^^
///     // Ownership of the application is transferred so it can be accessed
///     // after the request body future resolves.
///     //
///     // This is correct so long as `app` is dropped before the function
///     // returns.
///
///     // Deserialize a NewUser struct from the JSON request body.
///     // Then, convert it to a User struct with a generated UUID.
///     let user: User = future.await?.json::<NewUser>()?.into();
///
///     // Acquire a write lock on the list of users and insert a clone.
///     // Dropping the lock as soon as we can beats the cost of memcpy.
///     app.users.write().await.push(user.clone());
///
///     // 201 Created!
///     Response::build().status(201).json(&user)
/// }
/// ```
///
/// See: [`request.into_future()`] and [`request.into_parts()`].
///
/// ## Detached Tasks and Atomic Contention
///
/// `Shared<App>` relies on an atomic reference count to track ownership across
/// threads. In typical request handling, the clone/drop rhythm closely follows
/// the request lifecycle. This predictable cadence helps keep **atomic
/// contention low**.
///
/// Contention can be understood as waves:
///
/// - Each request incrementing or decrementing the reference count forms a peak
/// - If all requests align perfectly, peaks add together, increasing contention
/// - In practice, requests are staggered in time, causing the peaks to partially
///   cancel and flatten
///
/// ```text
/// 'process: ──────────────────────────────────────────────────────────────────────────>
///            |                             |                              |
///        HTTP GET                          |                              |
///       app.clone()                        |                              |
///    incr strong_count                 HTTP GET                           |
///            |                        app.clone()                         |
///            |                     incr strong_count                  HTTP POST
///        List Users                        |                         app.clone()
/// ┌──────────────────────┐                 |                      incr strong_count
/// |   borrow req.app()   |        Web Socket Upgrade                      |
/// |  acquire connection  |      ┌─────────────────────┐                   |
/// |   respond with json  |      |     app_owned()     |              Create User
/// └──────────────────────┘      |   spawn async task  |─┐     ┌──────────────────────┐
///     decr strong_count         | switching protocols | |     |   req.into_future()  |
///            |                  └─────────────────────┘ |     |     database trx     |
///            |                     decr strong_count    |     |       respond        |
///            |                             |            |     └──────────────────────┘
///            |                             |            |        decr strong_count
///            |                             |            |                 |
///            |                             |            └─>┌──────────────┐
///            |                             |               |  web socket  |
///            |                             |               └──────────────┘
///            |                             |               decr strong_count
///            |                             |                              |
/// ┌──────────|─────────────────────────────|──────────────────────────────|───────────┐
/// | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | | |
/// └──────────|─────────────────────────────|──────────────────────────────|───────────┘
///            |                             |                              |
///       uncontended                   uncontended                     contended
/// ```
///
/// Detached tasks disrupt this rhythm:
///
/// - Their increments and decrements occur out of phase with the request
///   lifecycle
/// - This can temporarily spike contention and extend resource lifetimes beyond
///   the request
///
/// Keeping `Shared<App>` clones phase-aligned with the request lifecycle
/// minimizes atomic contention and keeps resource lifetimes predictable. When
/// the Arc is dropped deterministically as the middleware future resolves,
/// leaks and unintended retention become significantly easier to detect.
///
/// #### Timing and Side-Channel Awareness
///
/// Each clone and drop of `Shared<App>` performs an atomic operation. When these
/// operations occur out of phase with normal request handling (for example, in
/// detached background tasks), they can introduce observable timing differences.
///
/// In high-assurance systems, such differences may:
///
/// - Act as unintended signals to an attacker
/// - Reveal the presence of privileged handlers (e.g., [web socket upgrades])
/// - Correlate background activity with specific request types
///
/// In these cases, preserving a uniform request rhythm may be more valuable than
/// minimizing contention. These tradeoffs should be made deliberately and
/// documented, as they exchange throughput and modularity for reduced
/// observability.
///
/// ### Example
///
/// ```
/// # use serde::{Deserialize, Serialize};
/// # use tokio::io::{self, AsyncWriteExt, Sink};
/// # use tokio::sync::{Mutex, RwLock};
/// # use uuid::Uuid;
/// # use via::{Next, Request, Response};
/// #
/// # /// An imaginary analytics service.
/// # struct Telemetry(Mutex<Sink>);
/// #
/// # /// Our billion dollar application.
/// # struct Unicorn {
/// #     telemetry: Telemetry,
/// #     users: RwLock<Vec<User>>,
/// # }
/// #
/// # #[derive(Clone, Serialize)]
/// # struct User {
/// #     id: Uuid,
/// #     email: String,
/// #     username: String,
/// # }
/// #
/// # impl Telemetry {
/// #     async fn report(&self, message: String) -> io::Result<()> {
/// #         let mut guard = self.0.lock().await;
/// #
/// #         guard.write_all(message.as_bytes()).await?;
/// #         guard.flush().await
/// #     }
/// # }
/// #
/// async fn destroy_user(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
///     // Parse a UUID from the :user-id parameter in the request URI.
///     let id = request.param("user-id").parse::<Uuid>()?;
///
///     // This example favors anonymity over performance. Therefore, we clone
///     // app as early as we can.
///     //
///     // Any earlier and Uuid parse errors could become a DoS vector that
///     // rapidly clones app out-of-phase with other HTTP requests. Doing so
///     // would amplify contention peaks rather than cancelling them.
///     let app = request.app_owned();
///
///     let user = {
///         // Acquire a write lock on the list of users.
///         let mut guard = app.users.write().await;
///
///         // Find the user with id = :id and remove it from the list of users.
///         guard.pop_if(|user| &id == &user.id)
///     };
///
///     // The status code of the response is the only dependency of user other
///     // than the telemetry task. Compute it early so user can be moved into
///     // the spawned task.
///     let status = user.is_some().then_some(204).unwrap_or(404);
///
///     // Spawn a task that owns of all of its dependencies.
///     //
///     // *Note:*
///     //
///     // The condition that determines whether or not we should report to the
///     // destroy op must be computed inside of the spawned task. This avoids
///     // a signal (task spawn) that can be used to determine the success of
///     // the write op outside of the HTTP transaction.
///     tokio::spawn(async move {
///         if let Some(User { id, .. }) = user {
///             let message = format!("delete: resource = users, id = {}", id);
///             let _ = app.telemetry.report(message).await.inspect_err(|error| {
///                 if cfg!(debug_assertions) {
///                     eprintln!("error(telemetry): {}", error);
///                 }
///             });
///         }
///     });
///
///     Response::build().status(status).finish()
/// }
/// ```
///
/// [`request.into_future()`]: crate::Request::into_future
/// [`request.into_parts()`]: crate::Request::into_parts
/// [web socket upgrades]: ../src/via/ws/upgrade.rs.html#256-262
///
pub struct Shared<App>(Arc<App>);

impl<App> Shared<App> {
    pub(super) fn new(value: App) -> Self {
        Self(Arc::new(value))
    }
}

impl<App> AsRef<App> for Shared<App> {
    #[inline]
    fn as_ref(&self) -> &App {
        &self.0
    }
}

impl<App> Clone for Shared<App> {
    #[inline]
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<App> Debug for Shared<App> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Shared").finish()
    }
}

impl<App> Deref for Shared<App> {
    type Target = App;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
