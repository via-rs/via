// use via::request::params::PathParams;
use via::{Response, raise};

use crate::database::Id;
use crate::util::Body;
use crate::{Next, Request};

pub async fn index(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn create(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn show(request: Request, _: Next) -> via::Result {
    let id = request.param("user-id").parse::<Id>()?;
    let Some(user) = request.app().database().find_user(id).await? else {
        raise!(404, message = "not found")
    };

    Response::build().json(&Body::new(user))
}

pub async fn update(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn destroy(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}
