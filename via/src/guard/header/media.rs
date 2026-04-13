use http::header::CONTENT_TYPE;

use super::{Header, header};
use crate::guard::bytes::{StrictEq, eq};
use crate::guard::predicate::{Or, or};

pub fn json() -> Header<Or<(StrictEq, StrictEq)>> {
    header(
        CONTENT_TYPE,
        or((
            eq(b"application/json"),
            eq(b"application/json; charset=utf-8"),
        )),
    )
}
