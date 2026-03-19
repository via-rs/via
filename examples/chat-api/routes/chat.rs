use via::{Next, Request, raise};

pub async fn chat(_: Request, _: Next) -> via::Result {
    raise!(message = "todo!")
}
