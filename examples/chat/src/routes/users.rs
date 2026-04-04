use http::StatusCode;
use via::{Payload, Response, raise};

use crate::database::Identify;
use crate::database::models::NewUser;
use crate::util::{Authenticate, Body, Identity};
use crate::{Next, Request};

pub async fn index(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn create(request: Request, _: Next) -> via::Result {
    let (future, app) = request.into_future();
    let Body { data } = future.await?.json::<Body<NewUser>>()?;

    let user = app.database().insert_user(data).await?;
    let identity = Identity::new(*user.id());
    let mut response = Response::build().json(&Body::new(user))?;

    response.authenticate(app.secret(), Some(identity));

    Ok(response)
}

pub async fn show(request: Request, _: Next) -> via::Result {
    let id = request.param("user-id").parse()?;
    let Some(user) = request.app().database().find_user(id).await? else {
        raise!(404, message = "not found")
    };

    Response::build().json(&Body::new(user))
}

pub async fn update(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn destroy(request: Request, _: Next) -> via::Result {
    let id = request.param("user-id").parse()?;

    if request.app().database().delete_user(id).await?.is_some() {
        Response::build().status(StatusCode::NO_CONTENT).finish()
    } else {
        raise!(404, message = "not found")
    }
}
