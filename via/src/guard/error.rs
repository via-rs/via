use super::header::HeaderError;
use crate::error::Error;

#[derive(Debug)]
pub enum GuardError<'a> {
    Header(HeaderError<'a>),
    Match,
    Method,
    Not,
    Other(Error),
}
