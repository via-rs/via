use http::header::ACCEPT;

use super::tag::{self, Tag, TagNoCase, tag_no_case};
use super::{Header, header};
use crate::guard::ErrorKind;
use crate::guard::predicate::{Or, Predicate, or};

pub struct Accept<T>(Or<(TagNoCase, T)>);

pub fn has<T>(predicate: T) -> Header<Accept<T>> {
    header(ACCEPT, Accept(or((tag_no_case(b"*/*"), predicate))))
}

pub fn json() -> Header<Accept<Or<(Tag, Tag, Tag)>>> {
    has(tag::json())
}

impl<T> Predicate<[u8]> for Accept<T>
where
    T: Predicate<[u8]>,
{
    fn cmp(&self, value: &[u8]) -> Result<(), ErrorKind> {
        if value
            .split(|byte| *byte == b',')
            .any(|media| self.0.cmp(media.trim_ascii()).is_ok())
        {
            Ok(())
        } else {
            Err(ErrorKind::Match)
        }
    }
}
