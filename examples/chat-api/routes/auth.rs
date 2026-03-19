use serde::Deserialize;
use via::{Next, Request, raise};

#[derive(Deserialize)]
struct LoginParams {
    username: String,
}

pub async fn me(_: Request, _: Next) -> via::Result {
    raise!(401, message = "unauthorized.")
}

pub async fn login(_: Request, _: Next) -> via::Result {
    raise!(401, message = "unauthorized.")
}

pub async fn logout(_: Request, _: Next) -> via::Result {
    raise!(401, message = "unauthorized.")
}
