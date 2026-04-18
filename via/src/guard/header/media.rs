//! Value combinators for HTTP media types.

use super::{CaseSensitive, Tag, case_sensitive, tag};
use crate::guard::Predicate;

/// An essence string with an optional charset.
///
/// Essence strings are case insensitive by default. A charset param is both
/// optional and case insensitive.
pub struct Media<T = Tag>(T, Option<Tag>);

/// Match `"*/*"(; charset=*)?`.
pub fn all() -> Media<CaseSensitive> {
    media(case_sensitive(b"*/*"), None)
}

/// Match `"text/html"(; charset=utf-8)?`.
pub fn html() -> Media {
    media(tag(b"text/html"), Some(b"utf-8"))
}

/// Match `"application/json"(; charset=utf-8)?`.
pub fn json() -> Media {
    media(tag(b"application/json"), Some(b"utf-8"))
}

/// Match `"text/plain"(; charset=utf-8)?`.
pub fn text() -> Media {
    media(tag(b"text/plain"), Some(b"utf-8"))
}

/// The essence charset param match the input. If the predicate and input both
/// have a charset they must match.
pub fn media<T>(essence: T, charset: Option<&[u8]>) -> Media<T> {
    Media(essence, charset.map(tag))
}

/// The mime's essence and optional charset param match the input.
#[cfg(feature = "mime")]
pub fn mime(value: mime::Mime) -> Media {
    media(
        value.essence_str().as_bytes(),
        value.get_param("charset").map(|p| p.as_str().as_bytes()),
    )
}

fn charset(input: &[u8]) -> Option<&[u8]> {
    let input = input.trim_ascii();
    let position = input.iter().position(|b| *b == b'=')?;
    let (key, value) = input.split_at(position);

    if key.eq_ignore_ascii_case(b"charset") {
        Some(&value[1..])
    } else {
        None
    }
}

impl<T> Predicate<[u8]> for Media<T>
where
    for<'a> T: Predicate<[u8], Error<'a> = ()> + 'a,
{
    type Error<'a> = ();

    fn cmp<'a>(&'a self, input: &[u8]) -> Result<(), Self::Error<'a>> {
        let mut iter = input.split(|b| *b == b';');

        iter.next()
            .ok_or(())
            .and_then(|essence| self.0.cmp(essence))?;

        self.1
            .as_ref()
            .zip(iter.find_map(charset))
            .map_or(Ok(()), |(lhs, rhs)| lhs.cmp(rhs))
    }
}
