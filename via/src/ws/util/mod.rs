mod future;

#[cfg(any(feature = "aws-lc-rs", feature = "ring"))]
mod sha1;

pub use future::poll_immediate_no_wake;

#[cfg(any(feature = "aws-lc-rs", feature = "ring"))]
pub use sha1::{Base64EncodedDigest, sha1};
