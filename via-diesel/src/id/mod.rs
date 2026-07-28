#[cfg(not(feature = "uuid"))]
mod serial;

#[cfg(not(feature = "uuid"))]
pub use serial::*;

#[cfg(feature = "uuid")]
mod uuid;

#[cfg(feature = "uuid")]
pub use uuid::*;
