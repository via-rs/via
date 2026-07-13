via::resource!(app = Unicorn);

use serde::Serialize;
use via::request::PathParams;
use via::{Error, Response, ResultExt};
use via_diesel::LimitAndOffset;
use via_diesel::prelude::*;

use crate::models::ThreadWithUser;
use crate::models::thread::{by_channel, by_thread, is_thread};
use crate::routes::channels::Subscriber;
use crate::util::Id;
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

    // Parse an Option<Id> from the :reply-id path parameter.
    let reply_id = request.param("reply-id").ok_and_then(str::parse)?;

    // Get pagination params from the URI query.
    let keyset = request.query::<LimitAndOffset>()?;

    // Load a page of threads.
    let threads = {
        // Acquire a database connection.
        let mut connection = request.app().database().await?;

        // Borrow the channel id from `subscription`.
        let channel_id = subscription.channel().id();

        if let Some(reply_id) = reply_id.as_ref() {
            ThreadWithUser::query()
                .filter(by_channel(channel_id).and(by_thread(reply_id)))
                .page(keyset)
                .load(&mut connection)
                .await?
        } else {
            ThreadWithUser::query()
                .filter(by_channel(channel_id).and(is_thread()))
                .page(keyset)
                .load(&mut connection)
                .await?
        }
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

/// Retrieve a user by id.
///
/// Responds to `GET /users/:user-id`.
async fn show(request: Request, _: Next) -> via::Result {
    let _params = request.params::<ThreadParams>()?;

    todo!()
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
