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
/// ```no_run
/// # mod models {
/// #     use diesel::prelude::*;
/// #     use serde::Serialize;
/// #     use uuid::Uuid;
/// #
/// #     diesel::table! {
/// #        users (id) {
/// #            id -> Uuid,
/// #            email -> Text,
/// #            username -> Text,
/// #        }
/// #     }
/// #
/// #     #[derive(Clone, Queryable, Selectable, Serialize)]
/// #     pub struct User {
/// #         id: Uuid,
/// #         email: String,
/// #         username: String,
/// #     }
/// # }
/// #
/// use bb8::{ManageConnection, Pool};
/// use diesel::prelude::*;
/// use diesel_async::{AsyncPgConnection, RunQueryDsl};
/// use diesel_async::pooled_connection::AsyncDieselConnectionManager;
/// use http::StatusCode;
/// use std::process::ExitCode;
/// use tokio::io::{self, AsyncWriteExt, Sink};
/// use tokio::sync::Mutex;
/// use uuid::Uuid;
/// use via::request::Payload;
/// use via::{Error, Next, Request, Response, Server};
///
/// use models::{users, User};
///
/// /// An imaginary analytics service.
/// struct Telemetry(Mutex<Sink>);
///
/// /// Our billion dollar application.
/// struct Unicorn {
///     database: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
///     telemetry: Telemetry,
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
/// impl Unicorn {
///     fn new() -> Self {
///         unimplemented!()
///     }
/// }
///
/// async fn find_user(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
///     let id = request.envelope().param("user-id").parse::<Uuid>()?;
///
///     // Acquire a database connection and find the user.
///     let user = users::table
///         .select(User::as_select())
///         .filter(users::id.eq(id))
///         .first(&mut request.app().database.get().await?)
///         .await?;
///
///     Response::build().json(&user)
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
/// # mod models {
/// #     use diesel::prelude::*;
/// #     use serde::{Deserialize, Serialize};
/// #     use uuid::Uuid;
/// #
/// #     diesel::table! {
/// #        users (id) {
/// #            id -> Uuid,
/// #            email -> Text,
/// #            username -> Text,
/// #        }
/// #     }
/// #
/// #     #[derive(Deserialize, Insertable)]
/// #     #[diesel(table_name = users)]
/// #     pub struct NewUser {
/// #         email: String,
/// #         username: String,
/// #     }
/// #
/// #     #[derive(Clone, Queryable, Selectable, Serialize)]
/// #     pub struct User {
/// #         id: Uuid,
/// #         email: String,
/// #         username: String,
/// #     }
/// # }
/// #
/// # use bb8::{ManageConnection, Pool};
/// # use diesel::prelude::*;
/// # use diesel_async::{AsyncPgConnection, RunQueryDsl};
/// # use diesel_async::pooled_connection::AsyncDieselConnectionManager;
/// # use http::StatusCode;
/// # use via::request::Payload;
/// # use via::{Next, Request, Response};
/// #
/// # use models::{users, NewUser, User};
/// #
/// # /// Our billion dollar application.
/// # struct Unicorn {
/// #     database: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
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
///     let user = diesel::insert_into(users::table)
///         .values(future.await?.json::<NewUser>()?)
///         .returning(User::as_returning())
///         .get_result(&mut app.database.get().await?)
///         .await?;
///
///     Response::build()
///         .status(StatusCode::CREATED)
///         .json(&user)
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
/// ### Example
///
/// ```
/// # mod models {
/// #     use uuid::Uuid;
/// #
/// #     diesel::table! {
/// #        users (id) {
/// #            id -> Uuid,
/// #            email -> Text,
/// #            username -> Text,
/// #        }
/// #     }
/// # }
/// #
/// # use bb8::{ManageConnection, Pool};
/// # use diesel::prelude::*;
/// # use diesel_async::{AsyncPgConnection, RunQueryDsl};
/// # use diesel_async::pooled_connection::AsyncDieselConnectionManager;
/// # use http::StatusCode;
/// # use tokio::io::{self, Sink};
/// # use tokio::sync::Mutex;
/// # use uuid::Uuid;
/// # use via::request::Payload;
/// # use via::{Next, Request, Response};
/// #
/// # use models::users;
/// #
/// # struct Telemetry(Mutex<Sink>);
/// #
/// # struct Unicorn {
/// #     database: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
/// #     telemetry: Telemetry,
/// # }
/// #
/// # impl Telemetry {
/// #     async fn report(&self, message: String) -> io::Result<()> { todo!() }
/// # }
/// #
/// async fn destroy_user(request: Request<Unicorn>, _: Next<Unicorn>) -> via::Result {
///     let id = request.param("user-id").parse::<Uuid>()?;
///
///     // Acquire a database connection and delete the user.
///     diesel::delete(users::table)
///         .filter(users::id.eq(id))
///         .execute(&mut request.app().database.get().await?)
///         .await?;
///
///     // Spawn a task that takes ownership of all of its dependencies.
///     tokio::spawn({
///         let app = request.app_owned();
///         let message = format!("delete: resource = users, id = {}", &id);
///         async move { app.telemetry.report(message).await }
///     });
///
///     Response::build()
///         .status(StatusCode::NO_CONTENT)
///         .finish()
/// }
/// ```
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
