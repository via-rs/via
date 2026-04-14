use http::HeaderName;

use crate::Error;

#[derive(Debug)]
pub enum ErrorKind {
    Header(HeaderName),
    Match,
    Method,
    Not,
    Other(Error),
}
