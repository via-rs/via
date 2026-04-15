use http::HeaderName;

use crate::Error;

#[derive(Debug)]
pub enum GuardError<'a> {
    Header(&'a HeaderName),
    Match,
    Method,
    Not,
    Other(Error),
}
