// use via::request::params::PathParams;
use via::deny;

use crate::{Next, Request};

pub async fn index(_: Request, _: Next) -> via::Result {
    deny!(500, "todo!")
}

pub async fn create(_: Request, _: Next) -> via::Result {
    deny!(500, "todo!")
}

pub async fn show(_: Request, _: Next) -> via::Result {
    deny!(500, "todo!")
}

pub async fn update(_: Request, _: Next) -> via::Result {
    deny!(500, "todo!")
}

pub async fn destroy(_: Request, _: Next) -> via::Result {
    deny!(500, "todo!")
}
