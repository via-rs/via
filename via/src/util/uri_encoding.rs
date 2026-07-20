use percent_encoding::percent_decode_str;
use std::borrow::Cow;

use crate::Error;

/// URI decoding mode for path and query parameter access.
#[derive(Clone, Copy, Debug)]
pub enum UriEncoding {
    /// Percent-decode the component before returning it.
    Percent,

    /// Return the component as it appeared in the URI.
    Unencoded,
}

impl UriEncoding {
    /// Decode `input` using this encoding mode.
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
