use http::header::CONTENT_TYPE;

use super::{Header, Tag, header, tag};
use crate::guard::Or;

pub fn json() -> Header<Or<(Tag, Tag, Tag)>> {
    header(CONTENT_TYPE, tag::json())
}
