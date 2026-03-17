use percent_encoding::percent_decode_str;
use std::borrow::Cow;

use crate::Error;

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
            percent_decode_str(input)
                .decode_utf8()
                .map_err(|_| Error::invalid_utf8_sequence(name))
        }
    }
}
