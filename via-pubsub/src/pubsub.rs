use std::future::Future;
use via::error::Catch;

use crate::sign::{OurEvent, PeerEvent};

pub type Result<T = ()> = std::result::Result<T, Catch>;

pub trait Dispatch<T, U> {
    type Subscription: Publish<OurEvent<T, U>> + Receive<Event = PeerEvent<T>>;

    fn dispatch(&self, event: OurEvent<T, U>);
    fn subscribe(&self, actor: T) -> Self::Subscription;
}

pub trait Publish<T> {
    fn send(&self, event: T) -> impl Future<Output = Result> + Send;
}

pub trait Receive {
    type Event;

    fn recv(&mut self) -> impl Future<Output = Result<Option<Self::Event>>> + Send;
    fn try_recv(&mut self) -> Result<Option<Self::Event>>;
}
