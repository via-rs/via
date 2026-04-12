use http::HeaderName;

use crate::error::BoxError;

#[derive(Debug)]
pub enum Deny {
    Header(HeaderName),
    Match,
    Method,
    Not,
    Other(BoxError),
}
