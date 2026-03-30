// use via::request::params::PathParams;
use via::raise;

use crate::{Next, Request};

pub async fn index(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn create(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn show(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn update(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}

pub async fn destroy(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}
