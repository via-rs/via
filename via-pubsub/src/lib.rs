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

pub mod backend;

mod pubsub;
mod util;

pub use backend::{Event, PeerEvent};
pub use pubsub::{Pubsub, Subscription};
pub use util::opaque::Opaque;
