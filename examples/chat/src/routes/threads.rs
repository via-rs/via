via::resource!(app = Unicorn);

use crate::{Next, Request, Unicorn};

async fn index(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

async fn create(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

async fn show(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

async fn update(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}

async fn destroy(_: Request, _: Next) -> via::Result {
    via::deny!(500, "todo!")
}
