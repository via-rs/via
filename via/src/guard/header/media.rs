use super::{Tag, TagNoCase, tag, tag_no_case};
use crate::guard::{Or, Predicate, or};

pub struct Media(Or<(Tag, TagNoCase)>, Option<TagNoCase>);

pub fn html() -> Media {
    media(b"text/html", Some(b"utf-8"))
}

pub fn json() -> Media {
    media(b"application/json", Some(b"utf-8"))
}

pub fn text() -> Media {
    media(b"text/plain", Some(b"utf-8"))
}

pub fn media(essence: &[u8], charset: Option<&[u8]>) -> Media {
    Media(
        or((tag(b"*/*"), tag_no_case(essence))),
        charset.map(tag_no_case),
    )
}

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

impl Predicate<[u8]> for Media {
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
