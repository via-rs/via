use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::Deserialize;
use via::deny;
use via::{Response, request::Payloadz};
use zeroize::Zeroizing;

use crate::database::users;
use crate::models::user::{User, by_id, by_username};
use crate::util::session::{Authenticate, Identity, Session};
use crate::{Next, Request};

#[derive(Deserialize)]
struct Login {
    username: String,
    password: Zeroizing<String>,
}

pub async fn me(request: Request, _: Next) -> via::Result {
    let Some(id) = request.session().map(Identity::id) else {
        deny!(401, "unauthorized");
    };

    let Some(user) = ({
        let mut conn = request.app().database().get().await?;

        users::table
            .select(User::as_select())
            .filter(by_id(&id))
            .first(&mut conn)
            .await
            .optional()?
    }) else {
        deny!(401, "unauthorized");
    };

    let mut response = Response::build().json(&user)?;
    response.authenticate(request.app().secret(), Some(Identity::new(&id)));

    Ok(response)
}

pub async fn login(request: Request, _: Next) -> via::Result {
    let (body, app) = request.into_future();
    let params = body.await?.be_z_data::<Login>()?;

    let Some(user) = ({
        let mut conn = app.database().get().await?;

        users::table
            .select(User::as_select())
            .filter(by_username(&params.username))
            .first(&mut conn)
            .await
            .optional()?
    }) else {
        deny!(401, "unauthorized");
    };

    let identity = Identity::new(user.id());
    let mut response = Response::build().data(user)?;

    response.authenticate(app.secret(), Some(identity));

    Ok(response)
}

pub async fn logout(request: Request, _: Next) -> via::Result {
    let mut response = Response::build().status(204).finish()?;

    response.authenticate(request.app().secret(), None);

    Ok(response)
}
