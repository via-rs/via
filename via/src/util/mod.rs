#[cfg(debug_assertions)]
mod once;
#[macro_use]
mod sealed;
mod uri_encoding;

#[cfg(debug_assertions)]
pub(crate) use once::once;
pub(crate) use sealed::sealed;

pub use uri_encoding::UriEncoding;
