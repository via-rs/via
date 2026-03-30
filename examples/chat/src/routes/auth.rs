use http::StatusCode;
use serde::Deserialize;
use via::{Payload, Response};

use crate::database::Identify;
use crate::util::session::Identity;
use crate::util::{Authenticate, Body, Session};
use crate::{Next, Request};

#[derive(Deserialize)]
struct Login {
    username: String,
}

pub async fn me(request: Request, _: Next) -> via::Result {
    Response::build().json(&Body::new(request.user().await?))
}

pub async fn login(request: Request, _: Next) -> via::Result {
    let (future, app) = request.into_future();
    let Body { data } = future.await?.bez_json::<Body<Login>>()?;

    if let Some(user) = app.database().fetch_user_by_username(data.username).await? {
        let identity = Identity::new(*user.id());
        let mut response = Response::build().json(&Body::new(user))?;

        response.authenticate(app.secret(), Some(identity));

        Ok(response)
    } else {
        Response::build().status(StatusCode::UNAUTHORIZED).finish()
    }
}

pub async fn logout(request: Request, _: Next) -> via::Result {
    let mut response = Response::build().status(StatusCode::NO_CONTENT).finish()?;

    response.authenticate(request.app().secret(), None);

    Ok(response)
}
