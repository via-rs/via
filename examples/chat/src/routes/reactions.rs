via::resource!(app = Unicorn, only = [collection, show, destroy]);

use diesel::{ExpressionMethods, Identifiable, QueryDsl};
use via::request::{PathParams, Payloadz};
use via::{Payload, Response, deny};
use via_diesel::paginate::{LimitAndOffset, LimitAndPage};
use via_diesel::{AsyncQueryDsl, Paginate};

use crate::models::ReactionWithUser;
use crate::models::reaction::{ReactionCollectionContext, ReactionMemberContext};
use crate::schema::reactions;
use crate::util::{Authenticator, Id, Session, hurry};
use crate::{Next, Request, Unicorn};

/// List reactions to a thread or reply.
async fn index(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let context = ReactionCollectionContext::try_from(&request)?;

    // Extract pagination arguments from the URI query.
    let by_number_with_limit = request.query::<LimitAndPage>()?;

    // Load a page of reactions to the thread or reply id in context.
    let reactions = hurry!(async {
        // Checkout a database connection.
        let mut connection = request.app().database().get().await?;

        // Execute the query.
        ReactionWithUser::query()
            .filter(reactions::thread_id.eq(context.thread_id()))
            .page(by_number_with_limit)
            .load_async(&mut connection)
            .await
    })?;

    Response::build().data(&reactions)
}

/// Create a new reaction to a thread.
async fn create(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let _context = ReactionCollectionContext::try_from(&request)?;

    todo!()
}

/// Retrieve a reaction to a thread or reply by id.
async fn show(request: Request, _: Next) -> via::Result {
    // Extract contextual arguments from the request.
    let _context = ReactionMemberContext::try_from(&request)?;

    todo!()
}

/// Delete a reaction by id.
async fn destroy(request: Request, _: Next) -> via::Result {
    let _context = ReactionMemberContext::try_from(&request)?;

    todo!()
}
