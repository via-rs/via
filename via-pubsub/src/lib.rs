macro_rules! log {
    ($level:tt($name:ident), $fmt:literal $($arg:tt)*) => {
        log!($level($name = 0), $fmt $($arg)*);
    };
    ($level:tt($name:ident = $indent:expr), $fmt:literal $(, $($arg:expr)+)?) => {
        $($(
            #[cfg(not(debug_assertions))]
            let _ = $arg;
        )*)?

        #[cfg(debug_assertions)]
        eprintln!(
            "{:indent$}{}({}): {}",
            "",
            stringify!($level),
            stringify!($name),
            format_args!($fmt $(, $($arg),+)?),
            indent = $indent * 2,
        );
    };
    ($level:tt($name:ident = $indent:expr, ret = $ret:expr), $fmt:literal $($arg:tt)*) => {{
        log!($level($name = $indent), $fmt $($arg)*);
        $ret
    }};
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
