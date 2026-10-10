macro_rules! log {
    ($level:tt, $fmt:literal $(, $($arg:expr),+)?) => {
        #[cfg(debug_assertions)]
        eprintln!(
            "{}(pubsub): {}",
            stringify!($level),
            format_args!($fmt $(, $($arg),*)?),
        );
    };
}

#[cfg(feature = "redis")]
pub mod redis;

mod error;
mod pubsub;
mod scope;
mod sign;

pub use pubsub::{Dispatch, Publish, Receive, Result};
pub use scope::Scope;
pub use sign::{Opaque, OurEvent, PeerEvent};

#[cfg(feature = "redis")]
pub use redis::Redis;
