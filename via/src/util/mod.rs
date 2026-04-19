#[cfg(debug_assertions)]
mod once;
mod uri_encoding;

#[cfg(debug_assertions)]
pub(crate) use once::once;

pub use uri_encoding::UriEncoding;
