via::resource!(app = Unicorn);

use diesel::prelude::*;
use serde::Serialize;
use via::request::PathParams;
use via::{Error, Response, ResultExt};
use via_diesel::paginate::{Keyset, LimitAndOffset, Paginate};
use via_diesel::{AsyncQueryDsl, Id};

use crate::models::thread::{by_channel, by_id, by_thread, is_thread};
use crate::models::{Thread, ThreadWithUser};
use crate::routes::channels::Subscriber;
use crate::schema::threads;
use crate::{Next, Request, Unicorn};

#[derive(Clone, Debug, Serialize)]
pub struct ThreadParams {
    thread_id: Id,
    reply_id: Option<Id>,
}

/// List threads.
///
/// Responds to:
/// - `GET /api/channels/:channel-id/threads`
/// - `GET /api/channels/:channel-id/threads/:thread-id/replies`
async fn index(request: Request, _: Next) -> via::Result {
    // Clone the subscription we loaded during authorization.
    let subscription = request.channel().cloned().or_not_found()?;

    // Parse an Option<Id> from the :thread-id path parameter.
    let parent_id = request.param("thread-id").ok_and_then(str::parse)?;

    // Get pagination params from the URI query.
    let keyset = request.query::<Keyset>()?;

    // Load a page of threads.
    let threads = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;
        let mut query = ThreadWithUser::query().page({
            //
            keyset.of(threads::created_at, threads::id)
        });

        if let Some(thread_id) = parent_id {
            query = query.filter(by_thread(thread_id));
        } else {
            query = query.filter(by_channel(subscription.channel_id()).and(is_thread()));
        }

        query.load_async(&mut connection).await?
    };

    Response::build().data(threads)
}

/// Create a new thread or reply to a thread.
///
/// Responds to:
/// - `POST /api/channels/:channel-id/threads`
/// - `POST /api/channels/:channel-id/threads/:thread-id/replies`
async fn create(_: Request, _: Next) -> via::Result {
    todo!()
}

/// Retrieve a thread or reply by id.
///
/// Responds to:
/// - `GET /api/channels/:channel-id/threads/:thread-id`
/// - `GET /api/channels/:channel-id/threads/:thread-id/replies/:reply-id`
async fn show(request: Request, _: Next) -> via::Result {
    // Parse an Option<Id> from the :thread-id path parameter.
    let params = request.params::<ThreadParams>()?;

    // Get pagination params from the URI query.
    let _keyset = request.query::<LimitAndOffset>()?;

    let thread = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        let id = params.reply_id.unwrap_or(params.thread_id);

        Thread::find(&mut connection, id).await?
    };

    Response::build().data(thread)
}

/// Update an existing user.
///
/// Responds to `PATCH /users/:user-id`.
///
/// The active user must be the user identified by `:user-id`.
async fn update(_: Request, _: Next) -> via::Result {
    todo!()
}

/// Delete a user account.
///
/// Responds to `DELETE /users/:user-id`.
///
/// The active user must be the user identified by `:user-id`.
async fn destroy(_: Request, _: Next) -> via::Result {
    todo!()
}

impl<'a> TryFrom<PathParams<'a>> for ThreadParams {
    type Error = Error;

    fn try_from(params: PathParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            thread_id: params.get("thread-id").parse()?,
            reply_id: params.get("reply-id").ok_and_then(str::parse)?,
        })
    }
}
