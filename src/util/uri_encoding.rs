use percent_encoding::percent_decode_str;
use std::borrow::Cow;

use crate::{Error, raise};

#[derive(Clone, Copy, Debug)]
pub enum UriEncoding {
    Percent,
    Unencoded,
}

impl UriEncoding {
    pub fn decode_as<'a>(&self, name: &str, input: &'a str) -> Result<Cow<'a, str>, Error> {
        if let Self::Unencoded = *self {
            Ok(Cow::Borrowed(input))
        } else {
            percent_decode_str(input).decode_utf8().or_else(|_| {
                raise!(
                    400,
                    message = format!("invalid utf-8 sequence of bytes in \"{}\"", name),
                )
            })
        }
    }
}
