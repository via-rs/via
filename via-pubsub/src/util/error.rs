use std::ops::ControlFlow;
use via::error::{Catch, Error};

pub fn sender_dropped<T>(_: T) -> Catch {
    let message = "pubsub closed".to_owned();
    ControlFlow::Break(Error::new(message))
}
